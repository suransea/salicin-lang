use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command, ExitStatus};
use std::time::{SystemTime, UNIX_EPOCH};

use salicin_lang::lockfile::{
    generate_lockfile_at, generate_workspace_lockfile, parse_lockfile, portable_relative_path,
    write_lockfile_if_changed, LOCKFILE_NAME,
};
use salicin_lang::manifest::{
    load_dependency_graph, load_project, DependencyGraph, Edition, Manifest, ProjectManifest,
    Target, TargetKind, MANIFEST_FILE_NAME, SOURCE_FILE_EXTENSION,
};
use salicin_lang::modules::{is_valid_module_segment, PackageId, SourcePackage, SourceUnit};
use salicin_lang::{
    check_library_source, check_library_source_packages, check_source, check_source_packages,
    compile_library_source, compile_library_source_packages, compile_source,
    compile_source_packages, compile_test_source, compile_test_source_packages,
    formatter::format_source,
    incremental::{fingerprint_package_graph, IncrementalTarget},
};

const DEFAULT_ALLOCATOR_RUNTIME: &str = include_str!("../../runtime/allocator.c");

const HELP: &str = "Salicin compiler

Usage:
  salic build [path] [-p <package>] [--bin <name>] [--locked | --frozen] [-o <path>]
  salic check [path] [-p <package>] [--bin <name> | --lib] [--locked | --frozen]
  salic emit-ir [path] [-p <package>] [--bin <name> | --lib] [--locked | --frozen] [-o <path>]
  salic fingerprint [path] [-p <package>] [--bin <name> | --lib] [--locked | --frozen]
  salic fmt [path] [--check]
  salic run [path] [-p <package>] [--bin <name>] [--locked | --frozen] [-- <args>...]
  salic test [path] [-p <package>] [--bin <name>] [--locked | --frozen]
  salic [path] [-p <package>] [--bin <name>] [--locked | --frozen] [-o <path>]

Commands:
    build      Compile a Salicin binary target to a native executable
    check      Parse and type-check a source or package target
  emit-ir    Print LLVM IR, or write it to a file with -o
  fingerprint Print the stable incremental input fingerprint
  fmt        Format one source file or every source in the selected package
  run        Compile and run a program; arguments after -- go to the program
  test       Compile all test declarations into one runner and run it

Options:
  -o, --output <path>  Select the output path
      --check          Check formatting without writing (fmt only)
      --bin <name>     Select a binary target from salicin.toml
      --lib            Select the library target (check and emit-ir only)
  -p, --package <name> Select a workspace package
      --locked         Require salicin.lock to be present and current
      --frozen         Require the lockfile and forbid dependency network access
  -h, --help           Print this help
  -V, --version        Print the compiler version

Path may be a .sc file, a project directory, or a salicin.toml manifest.
When path is omitted, salic searches the current directory and its parents.
Without an explicit command, the input is treated as a build command.";

enum ParsedArgs {
    Help,
    Version,
    Action(Action),
}

enum Action {
    Build {
        input: Option<PathBuf>,
        package: Option<String>,
        lock_mode: LockMode,
        bin: Option<String>,
        output: Option<PathBuf>,
    },
    Check {
        input: Option<PathBuf>,
        package: Option<String>,
        lock_mode: LockMode,
        target: TargetSelection,
    },
    EmitIr {
        input: Option<PathBuf>,
        package: Option<String>,
        lock_mode: LockMode,
        target: TargetSelection,
        output: Option<PathBuf>,
    },
    Fingerprint {
        input: Option<PathBuf>,
        package: Option<String>,
        lock_mode: LockMode,
        target: TargetSelection,
    },
    Format {
        input: Option<PathBuf>,
        check: bool,
    },
    Run {
        input: Option<PathBuf>,
        package: Option<String>,
        lock_mode: LockMode,
        bin: Option<String>,
        args: Vec<OsString>,
    },
    Test {
        input: Option<PathBuf>,
        package: Option<String>,
        lock_mode: LockMode,
        bin: Option<String>,
    },
}

#[derive(Clone)]
enum TargetSelection {
    Default,
    Bin(String),
    Lib,
}

#[derive(Clone, Copy, Default)]
enum LockMode {
    #[default]
    Update,
    Locked,
    Frozen,
}

struct CompileArgs {
    input: Option<PathBuf>,
    package: Option<String>,
    lock_mode: LockMode,
    output: Option<PathBuf>,
    target: TargetSelection,
}

fn main() {
    process::exit(run_cli(env::args_os().skip(1).collect()));
}

fn run_cli(args: Vec<OsString>) -> i32 {
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("salic: {message}");
            eprintln!("Try 'salic --help' for usage.");
            return 2;
        }
    };

    match parsed {
        ParsedArgs::Help => {
            println!("{HELP}");
            0
        }
        ParsedArgs::Version => {
            println!("salic {}", env!("CARGO_PKG_VERSION"));
            0
        }
        ParsedArgs::Action(action) => execute(action),
    }
}

fn parse_args(args: Vec<OsString>) -> Result<ParsedArgs, String> {
    let Some(first) = args.first() else {
        return Ok(ParsedArgs::Action(Action::Build {
            input: None,
            package: None,
            lock_mode: LockMode::Update,
            bin: None,
            output: None,
        }));
    };

    if args.len() == 1 && (is(first, "-h") || is(first, "--help")) {
        return Ok(ParsedArgs::Help);
    }
    if args.len() == 1 && (is(first, "-V") || is(first, "--version")) {
        return Ok(ParsedArgs::Version);
    }

    if args.len() == 2
        && matches!(
            first.to_str(),
            Some("build" | "check" | "emit-ir" | "fingerprint" | "fmt" | "run" | "test")
        )
        && (is(&args[1], "-h") || is(&args[1], "--help"))
    {
        return Ok(ParsedArgs::Help);
    }

    let action = match first.to_str() {
        Some("build") => {
            let parsed = parse_compile_args("build", &args[1..], true, false)?;
            let bin = selection_bin(parsed.target, "build")?;
            Action::Build {
                input: parsed.input,
                package: parsed.package,
                lock_mode: parsed.lock_mode,
                bin,
                output: parsed.output,
            }
        }
        Some("check") => {
            let parsed = parse_compile_args("check", &args[1..], false, true)?;
            Action::Check {
                input: parsed.input,
                package: parsed.package,
                lock_mode: parsed.lock_mode,
                target: parsed.target,
            }
        }
        Some("emit-ir") => {
            let parsed = parse_compile_args("emit-ir", &args[1..], true, true)?;
            Action::EmitIr {
                input: parsed.input,
                package: parsed.package,
                lock_mode: parsed.lock_mode,
                target: parsed.target,
                output: parsed.output,
            }
        }
        Some("fingerprint") => {
            let parsed = parse_compile_args("fingerprint", &args[1..], false, true)?;
            Action::Fingerprint {
                input: parsed.input,
                package: parsed.package,
                lock_mode: parsed.lock_mode,
                target: parsed.target,
            }
        }
        Some("fmt") => {
            let (input, check) = parse_format_args(&args[1..])?;
            Action::Format { input, check }
        }
        Some("run") => parse_run(&args[1..])?,
        Some("test") => {
            let parsed = parse_compile_args("test", &args[1..], false, false)?;
            let bin = selection_bin(parsed.target, "test")?;
            Action::Test {
                input: parsed.input,
                package: parsed.package,
                lock_mode: parsed.lock_mode,
                bin,
            }
        }
        _ if starts_with_dash(first)
            && !matches!(
                first.to_str(),
                Some("-o" | "--output" | "--bin" | "-p" | "--package" | "--locked" | "--frozen")
            ) =>
        {
            return Err(format!("unknown option '{}'", first.to_string_lossy()));
        }
        _ => {
            let parsed = parse_compile_args("build", &args, true, false)?;
            let bin = selection_bin(parsed.target, "build")?;
            Action::Build {
                input: parsed.input,
                package: parsed.package,
                lock_mode: parsed.lock_mode,
                bin,
                output: parsed.output,
            }
        }
    };

    Ok(ParsedArgs::Action(action))
}

fn parse_format_args(args: &[OsString]) -> Result<(Option<PathBuf>, bool), String> {
    let mut input = None;
    let mut check = false;
    for argument in args {
        if is(argument, "--check") {
            if check {
                return Err("'fmt' accepts '--check' only once".into());
            }
            check = true;
        } else if starts_with_dash(argument) {
            return Err(format!(
                "unknown option '{}' for 'fmt'",
                argument.to_string_lossy()
            ));
        } else if input.is_some() {
            return Err(format!(
                "'fmt' accepts at most one input path; unexpected argument '{}'",
                argument.to_string_lossy()
            ));
        } else {
            input = Some(PathBuf::from(argument));
        }
    }
    Ok((input, check))
}

fn parse_compile_args(
    command: &str,
    args: &[OsString],
    allow_output: bool,
    allow_lib: bool,
) -> Result<CompileArgs, String> {
    let mut input = None;
    let mut output = None;
    let mut package = None;
    let mut lock_mode = LockMode::Update;
    let mut target = TargetSelection::Default;
    let mut index = 0;

    while index < args.len() {
        let argument = &args[index];
        if is(argument, "-o") || is(argument, "--output") {
            if !allow_output {
                return Err(format!("'{command}' does not accept an output path"));
            }
            if output.is_some() {
                return Err(format!("'{command}' accepts only one output path"));
            }
            index += 1;
            let Some(path) = args.get(index) else {
                return Err(format!(
                    "'{}' requires an output path",
                    argument.to_string_lossy()
                ));
            };
            output = Some(PathBuf::from(path));
        } else if is(argument, "-p") || is(argument, "--package") {
            if package.is_some() {
                return Err(format!("'{command}' accepts only one package selector"));
            }
            index += 1;
            let Some(name) = args.get(index).and_then(|value| value.to_str()) else {
                return Err(format!(
                    "'{}' requires a UTF-8 package name",
                    argument.to_string_lossy()
                ));
            };
            if name.is_empty() {
                return Err(format!(
                    "'{}' requires a non-empty package name",
                    argument.to_string_lossy()
                ));
            }
            package = Some(name.to_owned());
        } else if is(argument, "--locked") || is(argument, "--frozen") {
            if !matches!(lock_mode, LockMode::Update) {
                return Err(format!(
                    "'{command}' accepts only one of '--locked' or '--frozen'"
                ));
            }
            lock_mode = if is(argument, "--locked") {
                LockMode::Locked
            } else {
                LockMode::Frozen
            };
        } else if is(argument, "--bin") {
            if !matches!(target, TargetSelection::Default) {
                return Err(format!("'{command}' accepts only one target selector"));
            }
            index += 1;
            let Some(name) = args.get(index).and_then(|value| value.to_str()) else {
                return Err("'--bin' requires a UTF-8 target name".into());
            };
            if name.is_empty() {
                return Err("'--bin' requires a non-empty target name".into());
            }
            target = TargetSelection::Bin(name.to_owned());
        } else if is(argument, "--lib") {
            if !allow_lib {
                return Err(format!("'{command}' does not support '--lib'"));
            }
            if !matches!(target, TargetSelection::Default) {
                return Err(format!("'{command}' accepts only one target selector"));
            }
            target = TargetSelection::Lib;
        } else if starts_with_dash(argument) {
            return Err(format!(
                "unknown option '{}' for '{command}'",
                argument.to_string_lossy()
            ));
        } else if input.is_some() {
            return Err(format!(
                "'{command}' accepts at most one input path; unexpected argument '{}'",
                argument.to_string_lossy()
            ));
        } else {
            input = Some(PathBuf::from(argument));
        }
        index += 1;
    }

    Ok(CompileArgs {
        input,
        package,
        lock_mode,
        output,
        target,
    })
}

fn selection_bin(selection: TargetSelection, command: &str) -> Result<Option<String>, String> {
    match selection {
        TargetSelection::Default => Ok(None),
        TargetSelection::Bin(name) => Ok(Some(name)),
        TargetSelection::Lib => Err(format!("'{command}' does not support '--lib'")),
    }
}

fn parse_run(args: &[OsString]) -> Result<Action, String> {
    let separator = args.iter().position(|argument| is(argument, "--"));
    let (compiler_args, program_args) = match separator {
        Some(index) => (&args[..index], args[index + 1..].to_vec()),
        None => (args, Vec::new()),
    };

    let parsed = parse_compile_args("run", compiler_args, false, false).map_err(|message| {
        if separator.is_none() && compiler_args.len() > 1 {
            format!("{message}; place program arguments after '--'")
        } else {
            message
        }
    })?;
    let bin = selection_bin(parsed.target, "run")?;
    Ok(Action::Run {
        input: parsed.input,
        package: parsed.package,
        lock_mode: parsed.lock_mode,
        bin,
        args: program_args,
    })
}

fn execute(action: Action) -> i32 {
    match action {
        Action::Build {
            input,
            package,
            lock_mode,
            bin,
            output,
        } => {
            let target = match resolve_input(
                input.as_deref(),
                package.as_deref(),
                lock_mode,
                bin.map_or(TargetSelection::Default, TargetSelection::Bin),
                true,
            ) {
                Ok(target) => target,
                Err(message) => return report_driver_error(message),
            };
            let output = match output {
                Some(output) => output,
                None => match target.default_output_path() {
                    Ok(output) => output,
                    Err(message) => {
                        eprintln!("salic: {message}");
                        return 2;
                    }
                },
            };
            if let Err(message) = target.ensure_distinct_output(&output) {
                eprintln!("salic: {message}");
                return 2;
            }

            let ir = match compile_target(&target) {
                Ok(ir) => ir,
                Err(()) => return 1,
            };
            match native_build(&ir, &output) {
                Ok(()) => 0,
                Err(message) => {
                    eprintln!("salic: {message}");
                    1
                }
            }
        }
        Action::Check {
            input,
            package,
            lock_mode,
            target,
        } => {
            let target = match resolve_input(
                input.as_deref(),
                package.as_deref(),
                lock_mode,
                target,
                false,
            ) {
                Ok(target) => target,
                Err(message) => return report_driver_error(message),
            };
            match check_target(&target) {
                Ok(()) => 0,
                Err(()) => 1,
            }
        }
        Action::EmitIr {
            input,
            package,
            lock_mode,
            target,
            output,
        } => {
            let target = match resolve_input(
                input.as_deref(),
                package.as_deref(),
                lock_mode,
                target,
                false,
            ) {
                Ok(target) => target,
                Err(message) => return report_driver_error(message),
            };
            if let Some(path) = output.as_deref() {
                if path != Path::new("-") {
                    if let Err(message) = target.ensure_distinct_output(path) {
                        eprintln!("salic: {message}");
                        return 2;
                    }
                }
            }
            let ir = match compile_target(&target) {
                Ok(ir) => ir,
                Err(()) => return 1,
            };
            match emit_ir(&ir, output.as_deref()) {
                Ok(()) => 0,
                Err(message) => {
                    eprintln!("salic: {message}");
                    1
                }
            }
        }
        Action::Fingerprint {
            input,
            package,
            lock_mode,
            target,
        } => {
            let target = match resolve_input(
                input.as_deref(),
                package.as_deref(),
                lock_mode,
                target,
                false,
            ) {
                Ok(target) => target,
                Err(message) => return report_driver_error(message),
            };
            match fingerprint_target(&target) {
                Ok(fingerprint) => {
                    println!("{fingerprint}");
                    0
                }
                Err(()) => 1,
            }
        }
        Action::Format { input, check } => match format_input(input.as_deref(), check) {
            Ok(code) => code,
            Err(message) => report_driver_error(message),
        },
        Action::Run {
            input,
            package,
            lock_mode,
            bin,
            args,
        } => {
            let target = match resolve_input(
                input.as_deref(),
                package.as_deref(),
                lock_mode,
                bin.map_or(TargetSelection::Default, TargetSelection::Bin),
                true,
            ) {
                Ok(target) => target,
                Err(message) => return report_driver_error(message),
            };
            let ir = match compile_target(&target) {
                Ok(ir) => ir,
                Err(()) => return 1,
            };
            match compile_and_run(&ir, &args) {
                Ok(code) => code,
                Err(message) => {
                    eprintln!("salic: {message}");
                    1
                }
            }
        }
        Action::Test {
            input,
            package,
            lock_mode,
            bin,
        } => {
            let target = match resolve_input(
                input.as_deref(),
                package.as_deref(),
                lock_mode,
                bin.map_or(TargetSelection::Default, TargetSelection::Bin),
                true,
            ) {
                Ok(target) => target,
                Err(message) => return report_driver_error(message),
            };
            let compilation = match compile_test_target(&target) {
                Ok(compilation) => compilation,
                Err(()) => return 1,
            };
            match compile_and_run(&compilation.ir, &[]) {
                Ok(0) => 0,
                Ok(index) => {
                    let name = usize::try_from(index)
                        .ok()
                        .and_then(|index| index.checked_sub(1))
                        .and_then(|index| compilation.names.get(index));
                    if let Some(name) = name {
                        eprintln!("salic: test {name:?} failed");
                    } else {
                        eprintln!("salic: test runner returned invalid failure index {index}");
                    }
                    1
                }
                Err(message) => {
                    eprintln!("salic: {message}");
                    1
                }
            }
        }
    }
}

struct ResolvedTarget {
    source: PathBuf,
    project: Option<ProjectTarget>,
    is_library: bool,
    edition: Edition,
    protected_inputs: Vec<PathBuf>,
}

struct ProjectTarget {
    package_root: PathBuf,
    target_name: String,
    packages: Vec<ResolvedPackage>,
}

struct ResolvedPackage {
    id: PackageId,
    name: String,
    version: String,
    identity: String,
    is_primary: bool,
    dependencies: BTreeMap<String, PackageId>,
    source: PathBuf,
    module_sources: Vec<(PathBuf, Vec<String>)>,
}

impl ResolvedTarget {
    fn default_output_path(&self) -> Result<PathBuf, String> {
        if let Some(project) = &self.project {
            let directory = project.package_root.join("build");
            fs::create_dir_all(&directory).map_err(|error| {
                format!(
                    "could not create package build directory '{}': {error}",
                    directory.display()
                )
            })?;
            Ok(directory.join(executable_name(&project.target_name)))
        } else {
            default_output_path(&self.source)
        }
    }

    fn ensure_distinct_output(&self, output: &Path) -> Result<(), String> {
        for input in &self.protected_inputs {
            if paths_refer_to_same_file(input, output) {
                return Err(format!(
                    "refusing to overwrite input file '{}'",
                    input.display()
                ));
            }
        }
        Ok(())
    }
}

fn report_driver_error(message: String) -> i32 {
    eprintln!("salic: {message}");
    2
}

fn resolve_input(
    input: Option<&Path>,
    package: Option<&str>,
    lock_mode: LockMode,
    selection: TargetSelection,
    binary_only: bool,
) -> Result<ResolvedTarget, String> {
    if let Some(path) = input {
        if path.extension() == Some(OsStr::new(SOURCE_FILE_EXTENSION)) {
            if package.is_some()
                || !matches!(lock_mode, LockMode::Update)
                || !matches!(selection, TargetSelection::Default)
            {
                return Err(
                    "package, lockfile, and target selectors require a salicin.toml input".into(),
                );
            }
            return Ok(ResolvedTarget {
                source: path.to_path_buf(),
                project: None,
                is_library: false,
                edition: Edition::Edition2026,
                protected_inputs: vec![path.to_path_buf()],
            });
        }
    }

    let manifest_path = match input {
        Some(path) if path.is_dir() => path.join(MANIFEST_FILE_NAME),
        Some(path) if path.file_name() == Some(OsStr::new(MANIFEST_FILE_NAME)) => {
            path.to_path_buf()
        }
        Some(path) if !path.exists() && path.extension().is_none() => path.join(MANIFEST_FILE_NAME),
        Some(path) => {
            return Err(format!(
                "input '{}' must be a .sc file, a package directory, or {MANIFEST_FILE_NAME}",
                path.display()
            ));
        }
        None => find_manifest_from_current_dir()?,
    };

    let mut project = load_project(&manifest_path).map_err(|error| error.to_string())?;
    let mut implicit_package = None;
    if let ProjectManifest::Package(manifest) = &project {
        if let Some(workspace) = find_containing_workspace(manifest)? {
            implicit_package = Some(manifest.package.name.clone());
            project = ProjectManifest::Workspace(workspace);
        }
    }
    let selected_package = package.or(implicit_package.as_deref());
    let (package_manifest, project_root) = select_project_package(&project, selected_package)?;
    let graph = load_dependency_graph(&package_manifest.manifest_path)
        .map_err(|error| error.to_string())?;
    let lock_graph = workspace_lock_graph(&project, &graph)?;
    let selected = select_manifest_target(graph.root(), selection, binary_only)?;
    let packages = resolve_project_packages(&graph, &selected, &project, &project_root)?;
    let lockfile_path = project_root.join(LOCKFILE_NAME);
    let workspace_members = match &project {
        ProjectManifest::Package(_) => HashSet::new(),
        ProjectManifest::Workspace(workspace) => workspace
            .packages()
            .map(|member| member.package_root.clone())
            .collect(),
    };
    let lockfile = if workspace_members.is_empty() {
        generate_lockfile_at(&lock_graph, &project_root)
    } else {
        generate_workspace_lockfile(&lock_graph, &project_root, &workspace_members)
    };
    apply_lock_mode(&lockfile_path, &lockfile, lock_mode)?;

    let mut protected_inputs = Vec::new();
    for manifest in &lock_graph.packages {
        protected_inputs.push(manifest.manifest_path.clone());
        protected_inputs.extend(manifest.targets().map(|target| target.path.clone()));
    }
    for package in &packages {
        protected_inputs.push(package.source.clone());
        protected_inputs.extend(package.module_sources.iter().map(|(path, _)| path.clone()));
    }
    protected_inputs.push(lockfile_path);

    Ok(ResolvedTarget {
        source: selected.path,
        project: Some(ProjectTarget {
            package_root: project_root,
            target_name: selected.name,
            packages,
        }),
        is_library: selected.kind == TargetKind::Lib,
        edition: package_manifest.package.edition,
        protected_inputs,
    })
}

fn apply_lock_mode(
    path: &Path,
    expected: &salicin_lang::lockfile::Lockfile,
    mode: LockMode,
) -> Result<(), String> {
    match mode {
        LockMode::Update => write_lockfile_if_changed(path, expected)
            .map(|_| ())
            .map_err(|error| format!("could not write lockfile '{}': {error}", path.display())),
        LockMode::Locked | LockMode::Frozen => {
            let source = fs::read_to_string(path).map_err(|error| {
                format!(
                    "lockfile '{}' is required by {} but could not be read: {error}",
                    path.display(),
                    lock_mode_name(mode)
                )
            })?;
            let actual = parse_lockfile(&source).map_err(|error| {
                format!(
                    "lockfile '{}' is invalid under {}: {error}",
                    path.display(),
                    lock_mode_name(mode)
                )
            })?;
            if &actual != expected {
                return Err(format!(
                    "lockfile '{}' is out of date under {}; run without the flag to update it",
                    path.display(),
                    lock_mode_name(mode)
                ));
            }
            Ok(())
        }
    }
}

fn lock_mode_name(mode: LockMode) -> &'static str {
    match mode {
        LockMode::Update => "default resolution",
        LockMode::Locked => "--locked",
        LockMode::Frozen => "--frozen",
    }
}

fn find_containing_workspace(
    member: &Manifest,
) -> Result<Option<salicin_lang::manifest::Workspace>, String> {
    let mut directory = member.package_root.parent();
    while let Some(parent) = directory {
        let candidate = parent.join(MANIFEST_FILE_NAME);
        if candidate.is_file() {
            match load_project(&candidate).map_err(|error| error.to_string())? {
                ProjectManifest::Workspace(workspace)
                    if workspace
                        .packages()
                        .any(|package| package.manifest_path == member.manifest_path) =>
                {
                    return Ok(Some(workspace));
                }
                _ => {}
            }
        }
        directory = parent.parent();
    }
    Ok(None)
}

fn workspace_lock_graph(
    project: &ProjectManifest,
    selected: &DependencyGraph,
) -> Result<DependencyGraph, String> {
    let ProjectManifest::Workspace(workspace) = project else {
        return Ok(selected.clone());
    };
    let mut packages = BTreeMap::new();
    for member in workspace.packages() {
        let graph =
            load_dependency_graph(&member.manifest_path).map_err(|error| error.to_string())?;
        for manifest in graph.packages {
            packages.insert(manifest.manifest_path.clone(), manifest);
        }
    }
    Ok(DependencyGraph {
        root_manifest_path: selected.root_manifest_path.clone(),
        packages: packages.into_values().collect(),
    })
}

fn resolve_project_packages(
    graph: &DependencyGraph,
    selected: &Target,
    project: &ProjectManifest,
    project_root: &Path,
) -> Result<Vec<ResolvedPackage>, String> {
    let ids: HashMap<PathBuf, PackageId> = graph
        .packages
        .iter()
        .enumerate()
        .map(|(index, manifest)| (manifest.manifest_path.clone(), PackageId(index)))
        .collect();
    let mut packages = Vec::with_capacity(graph.packages.len());

    for manifest in &graph.packages {
        let is_primary = manifest.manifest_path == graph.root_manifest_path;
        let root_source = if is_primary {
            selected.path.clone()
        } else {
            manifest
                .lib
                .as_ref()
                .expect("dependency graph validation requires dependency libraries")
                .path
                .clone()
        };
        let target_sources = manifest
            .targets()
            .map(|target| target.path.clone())
            .collect::<Vec<_>>();
        let module_sources =
            discover_package_module_sources(&manifest.package_root, &target_sources, &root_source)?;
        let dependencies = manifest
            .dependencies
            .iter()
            .map(|dependency| {
                let target = ids
                    .get(&dependency.manifest_path)
                    .copied()
                    .expect("every validated dependency is present in the graph");
                (dependency.alias.clone(), target)
            })
            .collect();
        packages.push(ResolvedPackage {
            id: ids[&manifest.manifest_path],
            name: manifest.package.name.clone(),
            version: manifest.package.version.to_string(),
            identity: resolved_package_identity(project, project_root, manifest),
            is_primary,
            dependencies,
            source: root_source,
            module_sources,
        });
    }
    Ok(packages)
}

fn resolved_package_identity(
    project: &ProjectManifest,
    project_root: &Path,
    manifest: &Manifest,
) -> String {
    let workspace_member = match project {
        ProjectManifest::Package(_) => false,
        ProjectManifest::Workspace(workspace) => workspace
            .packages()
            .any(|member| member.manifest_path == manifest.manifest_path),
    };
    let category = if workspace_member {
        "workspace"
    } else {
        "path"
    };
    format!(
        "{category}:{}|{}@{}",
        portable_relative_path(project_root, &manifest.package_root),
        manifest.package.name,
        manifest.package.version
    )
}

fn discover_package_module_sources(
    package_root: &Path,
    target_sources: &[PathBuf],
    root_source: &Path,
) -> Result<Vec<(PathBuf, Vec<String>)>, String> {
    let source_root = package_root.join("src");
    if !source_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut candidates = Vec::new();
    collect_sc_files(&source_root, &mut candidates)?;
    candidates.sort();

    let mut modules = Vec::new();
    let mut seen = HashSet::new();
    for path in candidates {
        if paths_refer_to_same_file(&path, root_source)
            || target_sources
                .iter()
                .any(|other| paths_refer_to_same_file(&path, other))
        {
            continue;
        }
        let relative = path.strip_prefix(&source_root).map_err(|_| {
            format!(
                "source module '{}' is outside '{}'",
                path.display(),
                source_root.display()
            )
        })?;
        if relative == Path::new("main.sc")
            || relative == Path::new("lib.sc")
            || relative
                .components()
                .next()
                .is_some_and(|component| component.as_os_str() == OsStr::new("bin"))
        {
            continue;
        }

        let module_path = module_path_from_relative(relative)?;
        if !seen.insert(module_path.clone()) {
            return Err(format!("duplicate file module `{}`", module_path.join(".")));
        }
        modules.push((path, module_path));
    }
    Ok(modules)
}

fn collect_sc_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "could not read source directory '{}': {error}",
            directory.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "could not read an entry in source directory '{}': {error}",
                directory.display()
            )
        })?;
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "could not inspect source path '{}': {error}",
                entry.path().display()
            )
        })?;
        if file_type.is_dir() {
            collect_sc_files(&entry.path(), output)?;
        } else if file_type.is_file()
            && entry.path().extension() == Some(OsStr::new(SOURCE_FILE_EXTENSION))
        {
            output.push(entry.path());
        }
    }
    Ok(())
}

fn module_path_from_relative(relative: &Path) -> Result<Vec<String>, String> {
    let mut path = relative.to_path_buf();
    path.set_extension("");
    let mut modules = Vec::new();
    for component in path.components() {
        let Some(segment) = component.as_os_str().to_str() else {
            return Err(format!(
                "file module path '{}' must be valid UTF-8",
                relative.display()
            ));
        };
        if !is_valid_module_segment(segment) {
            return Err(format!(
                "file module segment `{segment}` in '{}' must be a non-reserved ASCII snake_case identifier",
                relative.display()
            ));
        }
        modules.push(segment.to_owned());
    }
    if modules.is_empty() {
        return Err(format!(
            "source file '{}' does not define a module path",
            relative.display()
        ));
    }
    if matches!(modules.first().map(String::as_str), Some("core" | "alloc")) {
        return Err(format!(
            "top-level file module `{}` in '{}' conflicts with the standard-library namespace",
            modules[0],
            relative.display()
        ));
    }
    Ok(modules)
}

fn find_manifest_from_current_dir() -> Result<PathBuf, String> {
    let mut directory = env::current_dir()
        .map_err(|error| format!("could not determine the current directory: {error}"))?;
    loop {
        let candidate = directory.join(MANIFEST_FILE_NAME);
        if candidate.is_file() {
            return Ok(candidate);
        }
        if !directory.pop() {
            return Err(format!(
                "could not find {MANIFEST_FILE_NAME} in the current directory or any parent"
            ));
        }
    }
}

fn format_input(input: Option<&Path>, check: bool) -> Result<i32, String> {
    let paths = resolve_format_paths(input)?;
    let mut formatted = Vec::with_capacity(paths.len());
    let mut invalid = false;
    for path in paths {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("could not read source file '{}': {error}", path.display()))?;
        match format_source(&source) {
            Ok(output) => formatted.push((path, source, output)),
            Err(error) => {
                eprintln!("{}:{error}", path.display());
                invalid = true;
            }
        }
    }
    if invalid {
        return Ok(1);
    }

    let changed = formatted
        .iter()
        .filter(|(_, source, output)| source != output)
        .collect::<Vec<_>>();
    if check {
        for (path, _, _) in &changed {
            eprintln!("salic: source is not formatted: {}", path.display());
        }
        return Ok(i32::from(!changed.is_empty()));
    }
    for (path, _, output) in changed {
        fs::write(path, output).map_err(|error| {
            format!(
                "could not write formatted source '{}': {error}",
                path.display()
            )
        })?;
    }
    Ok(0)
}

fn resolve_format_paths(input: Option<&Path>) -> Result<Vec<PathBuf>, String> {
    if let Some(path) = input {
        if path.extension() == Some(OsStr::new(SOURCE_FILE_EXTENSION)) {
            return Ok(vec![path.to_path_buf()]);
        }
    }

    let manifest_path = match input {
        Some(path) if path.is_dir() => path.join(MANIFEST_FILE_NAME),
        Some(path) if path.file_name() == Some(OsStr::new(MANIFEST_FILE_NAME)) => {
            path.to_path_buf()
        }
        Some(path) if !path.exists() && path.extension().is_none() => path.join(MANIFEST_FILE_NAME),
        Some(path) => {
            return Err(format!(
                "input '{}' must be a .sc file, a package directory, or {MANIFEST_FILE_NAME}",
                path.display()
            ));
        }
        None => find_manifest_from_current_dir()?,
    };
    let project = load_project(&manifest_path).map_err(|error| error.to_string())?;
    let mut paths = Vec::new();
    match &project {
        ProjectManifest::Package(package) => collect_manifest_source_paths(package, &mut paths)?,
        ProjectManifest::Workspace(workspace) => {
            for package in workspace.packages() {
                collect_manifest_source_paths(package, &mut paths)?;
            }
        }
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Err("selected package or workspace contains no Salicin source files".into());
    }
    Ok(paths)
}

fn collect_manifest_source_paths(
    manifest: &Manifest,
    paths: &mut Vec<PathBuf>,
) -> Result<(), String> {
    paths.extend(manifest.targets().map(|target| target.path.clone()));
    let source_root = manifest.package_root.join("src");
    if source_root.is_dir() {
        collect_sc_files(&source_root, paths)?;
    }
    Ok(())
}

fn select_project_package<'a>(
    project: &'a ProjectManifest,
    package: Option<&str>,
) -> Result<(&'a Manifest, PathBuf), String> {
    match project {
        ProjectManifest::Package(manifest) => {
            if let Some(requested) = package {
                if requested != manifest.package.name {
                    return Err(format!(
                        "package manifest defines `{}`, not requested package `{requested}`",
                        manifest.package.name
                    ));
                }
            }
            Ok((manifest, manifest.package_root.clone()))
        }
        ProjectManifest::Workspace(workspace) => {
            let selected = if let Some(requested) = package {
                workspace.package_named(requested).ok_or_else(|| {
                    let available = workspace
                        .packages()
                        .map(|member| member.package.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "workspace has no package named `{requested}`; available packages: {available}"
                    )
                })?
            } else if let Some(root) = workspace.root_package.as_ref() {
                root
            } else {
                match workspace.members.as_slice() {
                    [only] => only,
                    _ => {
                        return Err(
                            "virtual workspace has multiple packages; choose one with --package <name>"
                                .into(),
                        );
                    }
                }
            };
            Ok((selected, workspace.root.clone()))
        }
    }
}

fn select_manifest_target(
    manifest: &Manifest,
    selection: TargetSelection,
    binary_only: bool,
) -> Result<Target, String> {
    match selection {
        TargetSelection::Bin(name) => manifest
            .bins
            .iter()
            .find(|target| target.name == name)
            .cloned()
            .ok_or_else(|| format!("package has no binary target named `{name}`")),
        TargetSelection::Lib if binary_only => {
            Err("this command requires a binary target and does not support --lib".into())
        }
        TargetSelection::Lib => manifest
            .lib
            .clone()
            .ok_or_else(|| "package has no library target".into()),
        TargetSelection::Default if binary_only => select_default_binary(manifest),
        TargetSelection::Default => {
            if manifest.bins.is_empty() {
                manifest
                    .lib
                    .clone()
                    .ok_or_else(|| "package has no target".into())
            } else {
                select_default_binary(manifest)
            }
        }
    }
}

fn select_default_binary(manifest: &Manifest) -> Result<Target, String> {
    if let Some(target) = manifest
        .bins
        .iter()
        .find(|target| target.name == manifest.package.name)
    {
        return Ok(target.clone());
    }
    match manifest.bins.as_slice() {
        [] => Err("package has no binary target".into()),
        [target] => Ok(target.clone()),
        _ => Err("package has multiple binary targets; choose one with --bin <name>".into()),
    }
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    if let (Ok(left), Ok(right)) = (fs::canonicalize(left), fs::canonicalize(right)) {
        if left == right {
            return true;
        }
    }

    if same_file::is_same_file(left, right).unwrap_or(false) {
        return true;
    }

    false
}

fn read_source(source: &Path) -> Result<String, ()> {
    match fs::read_to_string(source) {
        Ok(text) => Ok(text),
        Err(error) => {
            eprintln!(
                "salic: could not read source file '{}': {error}",
                source.display()
            );
            Err(())
        }
    }
}

fn compile_file(source: &Path, library: bool) -> Result<String, ()> {
    let text = read_source(source)?;
    let result = if library {
        compile_library_source(&text)
    } else {
        compile_source(&text)
    };

    report_compilation(source, result)
}

fn compile_target(target: &ResolvedTarget) -> Result<String, ()> {
    if target.project.is_none() {
        return compile_file(&target.source, target.is_library);
    }

    let packages = read_source_packages(target)?;
    let result = if target.is_library {
        compile_library_source_packages(&packages)
    } else {
        compile_source_packages(&packages)
    };
    report_project_compilation(&target.source, result)
}

fn compile_test_target(target: &ResolvedTarget) -> Result<salicin_lang::TestCompilation, ()> {
    if target.project.is_none() {
        let text = read_source(&target.source)?;
        return report_compilation(&target.source, compile_test_source(&text));
    }

    let packages = read_source_packages(target)?;
    report_project_compilation(&target.source, compile_test_source_packages(&packages))
}

fn fingerprint_target(
    target: &ResolvedTarget,
) -> Result<salicin_lang::incremental::IncrementalFingerprint, ()> {
    let packages = if target.project.is_some() {
        read_source_packages(target)?
    } else {
        vec![SourcePackage {
            id: PackageId(0),
            name: "source".into(),
            version: "0.0.0".into(),
            identity: "source@0.0.0".into(),
            is_primary: true,
            dependencies: BTreeMap::new(),
            sources: vec![SourceUnit {
                path: target.source.display().to_string(),
                module_path: Vec::new(),
                source: read_source(&target.source)?,
                is_root: true,
            }],
        }]
    };
    let mode = if target.is_library {
        IncrementalTarget::Library
    } else {
        IncrementalTarget::Binary
    };
    fingerprint_package_graph(&packages, target.edition, mode).map_err(|error| {
        eprintln!("salic: could not fingerprint incremental inputs: {error}");
    })
}

fn check_file(source: &Path, library: bool) -> Result<(), ()> {
    let text = read_source(source)?;
    let result = if library {
        check_library_source(&text)
    } else {
        check_source(&text)
    };
    report_compilation(source, result)
}

fn check_target(target: &ResolvedTarget) -> Result<(), ()> {
    if target.project.is_none() {
        return check_file(&target.source, target.is_library);
    }

    let packages = read_source_packages(target)?;
    let result = if target.is_library {
        check_library_source_packages(&packages)
    } else {
        check_source_packages(&packages)
    };
    report_project_compilation(&target.source, result)
}

fn read_source_packages(target: &ResolvedTarget) -> Result<Vec<SourcePackage>, ()> {
    let project = target
        .project
        .as_ref()
        .expect("package source reading requires a resolved project");
    project
        .packages
        .iter()
        .map(|package| {
            let mut sources = Vec::with_capacity(package.module_sources.len() + 1);
            sources.push(SourceUnit {
                path: package.source.display().to_string(),
                module_path: Vec::new(),
                source: read_source(&package.source)?,
                is_root: true,
            });
            for (path, module_path) in &package.module_sources {
                sources.push(SourceUnit {
                    path: path.display().to_string(),
                    module_path: module_path.clone(),
                    source: read_source(path)?,
                    is_root: false,
                });
            }
            Ok(SourcePackage {
                id: package.id,
                name: package.name.clone(),
                version: package.version.clone(),
                identity: package.identity.clone(),
                is_primary: package.is_primary,
                dependencies: package.dependencies.clone(),
                sources,
            })
        })
        .collect()
}

fn report_compilation<T>(source: &Path, result: Result<T, Vec<String>>) -> Result<T, ()> {
    match result {
        Ok(value) => Ok(value),
        Err(diagnostics) => {
            if diagnostics.is_empty() {
                eprintln!("{}: error: compilation failed", source.display());
            } else {
                for diagnostic in diagnostics {
                    if diagnostic
                        .split_once(':')
                        .is_some_and(|(line, _)| line.bytes().all(|byte| byte.is_ascii_digit()))
                    {
                        eprintln!("{}:{diagnostic}", source.display());
                    } else {
                        eprintln!("{}: {diagnostic}", source.display());
                    }
                }
            }
            Err(())
        }
    }
}

fn report_project_compilation<T>(source: &Path, result: Result<T, Vec<String>>) -> Result<T, ()> {
    match result {
        Ok(value) => Ok(value),
        Err(diagnostics) => {
            if diagnostics.is_empty() {
                eprintln!("{}: error: compilation failed", source.display());
            } else {
                for diagnostic in diagnostics {
                    if diagnostic.starts_with("error:") {
                        // The semantic analyzer does not carry per-item source
                        // maps yet. Avoid attributing a module error to the
                        // package root merely because it owns the target.
                        eprintln!("salic: {diagnostic}");
                    } else {
                        eprintln!("{diagnostic}");
                    }
                }
            }
            Err(())
        }
    }
}

fn emit_ir(ir: &str, output: Option<&Path>) -> Result<(), String> {
    match output {
        None => io::stdout()
            .lock()
            .write_all(ir.as_bytes())
            .map_err(|error| format!("could not write LLVM IR to stdout: {error}")),
        Some(path) if path == Path::new("-") => io::stdout()
            .lock()
            .write_all(ir.as_bytes())
            .map_err(|error| format!("could not write LLVM IR to stdout: {error}")),
        Some(path) => fs::write(path, ir)
            .map_err(|error| format!("could not write LLVM IR to '{}': {error}", path.display())),
    }
}

fn native_build(ir: &str, output: &Path) -> Result<(), String> {
    let temporary = TemporaryDirectory::new()?;
    let ir_path = temporary.path().join("module.ll");
    let runtime_path = temporary.path().join("allocator.c");
    fs::write(&ir_path, ir).map_err(|error| {
        format!(
            "could not write temporary LLVM IR '{}': {error}",
            ir_path.display()
        )
    })?;
    write_allocator_runtime(&runtime_path)?;
    invoke_clang(&ir_path, &runtime_path, output)
}

fn compile_and_run(ir: &str, args: &[OsString]) -> Result<i32, String> {
    let temporary = TemporaryDirectory::new()?;
    let ir_path = temporary.path().join("module.ll");
    let runtime_path = temporary.path().join("allocator.c");
    let executable = temporary.path().join(executable_name("program"));

    fs::write(&ir_path, ir).map_err(|error| {
        format!(
            "could not write temporary LLVM IR '{}': {error}",
            ir_path.display()
        )
    })?;
    write_allocator_runtime(&runtime_path)?;
    invoke_clang(&ir_path, &runtime_path, &executable)?;

    let status = Command::new(&executable)
        .args(args)
        .status()
        .map_err(|error| format!("could not run '{}': {error}", executable.display()))?;
    Ok(program_exit_code(status))
}

fn write_allocator_runtime(path: &Path) -> Result<(), String> {
    fs::write(path, DEFAULT_ALLOCATOR_RUNTIME).map_err(|error| {
        format!(
            "could not write allocator runtime '{}': {error}",
            path.display()
        )
    })
}

fn invoke_clang(ir: &Path, runtime: &Path, output: &Path) -> Result<(), String> {
    let system_clang = Path::new("/usr/bin/clang");
    let compiler: &OsStr = if system_clang.is_file() {
        system_clang.as_os_str()
    } else {
        OsStr::new("clang")
    };

    let runtime_object = cached_allocator_runtime(compiler, runtime)?;
    let status = Command::new(compiler)
        .arg("-Wno-override-module")
        .arg("-x")
        .arg("ir")
        .arg(ir)
        .arg("-x")
        .arg("none")
        .arg(&runtime_object)
        .arg("-o")
        .arg(output)
        .status()
        .map_err(|error| {
            format!(
                "could not start LLVM linker '{}': {error}",
                Path::new(compiler).display()
            )
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("native compilation failed with {status}"))
    }
}

fn default_output_path(source: &Path) -> Result<PathBuf, String> {
    let stem = source.file_stem().ok_or_else(|| {
        format!(
            "cannot derive an output filename from '{}'",
            source.display()
        )
    })?;
    if stem.is_empty() {
        return Err(format!(
            "cannot derive an output filename from '{}'",
            source.display()
        ));
    }

    let mut output = source.with_file_name(stem);
    if !env::consts::EXE_EXTENSION.is_empty() {
        output.set_extension(env::consts::EXE_EXTENSION);
    }
    Ok(output)
}

fn executable_name(stem: &str) -> OsString {
    if env::consts::EXE_EXTENSION.is_empty() {
        OsString::from(stem)
    } else {
        OsString::from(format!("{stem}.{}", env::consts::EXE_EXTENSION))
    }
}

fn cached_allocator_runtime(compiler: &OsStr, source: &Path) -> Result<PathBuf, String> {
    let mut hasher = DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    env::consts::OS.hash(&mut hasher);
    env::consts::ARCH.hash(&mut hasher);
    compiler.hash(&mut hasher);
    DEFAULT_ALLOCATOR_RUNTIME.hash(&mut hasher);
    let cache_key = hasher.finish();
    let cache_directory = env::temp_dir().join("salic-runtime-cache");
    let cached = cache_directory.join(format!("allocator-{cache_key:016x}.o"));
    if cached.is_file() {
        return Ok(cached);
    }

    fs::create_dir_all(&cache_directory).map_err(|error| {
        format!(
            "could not create runtime cache '{}': {error}",
            cache_directory.display()
        )
    })?;
    let temporary = TemporaryDirectory::new()?;
    let object = temporary.path().join("allocator.o");
    let status = Command::new(compiler)
        .arg("-c")
        .arg("-x")
        .arg("c")
        .arg("-std=c11")
        .arg(source)
        .arg("-o")
        .arg(&object)
        .status()
        .map_err(|error| {
            format!(
                "could not compile allocator runtime '{}': {error}",
                source.display()
            )
        })?;
    if !status.success() {
        return Err(format!(
            "allocator runtime compilation failed with {status}"
        ));
    }

    match fs::rename(&object, &cached) {
        Ok(()) => Ok(cached),
        Err(_) if cached.is_file() => Ok(cached),
        Err(error) => Err(format!(
            "could not cache allocator runtime as '{}': {error}",
            cached.display()
        )),
    }
}

fn program_exit_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }

    1
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new() -> Result<Self, String> {
        let base = env::temp_dir();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let process_id = process::id();

        for attempt in 0..100_u32 {
            let path = base.join(format!("salic-{process_id}-{timestamp}-{attempt}"));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "could not create temporary directory '{}': {error}",
                        path.display()
                    ));
                }
            }
        }

        Err(format!(
            "could not allocate a unique temporary directory in '{}'",
            base.display()
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn is(argument: &OsStr, expected: &str) -> bool {
    argument == OsStr::new(expected)
}

fn starts_with_dash(argument: &OsStr) -> bool {
    argument.to_string_lossy().starts_with('-')
}
