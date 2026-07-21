use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command, ExitStatus};
use std::time::{SystemTime, UNIX_EPOCH};

use salicin_lang::lockfile::{write_package_lockfile, LOCKFILE_NAME};
use salicin_lang::manifest::{
    load_dependency_graph, DependencyGraph, Manifest, Target, TargetKind, MANIFEST_FILE_NAME,
};
use salicin_lang::modules::{is_valid_module_segment, PackageId, SourcePackage, SourceUnit};
use salicin_lang::{
    check_library_source, check_library_source_packages, check_source_packages,
    compile_library_source, compile_library_source_packages, compile_source,
    compile_source_packages,
};

const DEFAULT_ALLOCATOR_RUNTIME: &str = include_str!("../../runtime/allocator.c");

const HELP: &str = "Salicin compiler

Usage:
  salic build [path] [--bin <name>] [-o <path>]
  salic check [path] [--bin <name> | --lib]
  salic emit-ir [path] [--bin <name> | --lib] [-o <path>]
  salic run [path] [--bin <name>] [-- <args>...]
  salic [path] [--bin <name>] [-o <path>]

Commands:
    build      Compile a Salicin binary target to a native executable
    check      Parse and type-check a source or package target
  emit-ir    Print LLVM IR, or write it to a file with -o
  run        Compile and run a program; arguments after -- go to the program

Options:
  -o, --output <path>  Select the output path
      --bin <name>     Select a binary target from salicin.toml
      --lib            Select the library target (check and emit-ir only)
  -h, --help           Print this help
  -V, --version        Print the compiler version

Path may be a .sali file, a project directory, or a salicin.toml manifest.
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
        bin: Option<String>,
        output: Option<PathBuf>,
    },
    Check {
        input: Option<PathBuf>,
        target: TargetSelection,
    },
    EmitIr {
        input: Option<PathBuf>,
        target: TargetSelection,
        output: Option<PathBuf>,
    },
    Run {
        input: Option<PathBuf>,
        bin: Option<String>,
        args: Vec<OsString>,
    },
}

#[derive(Clone)]
enum TargetSelection {
    Default,
    Bin(String),
    Lib,
}

struct CompileArgs {
    input: Option<PathBuf>,
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
        && matches!(first.to_str(), Some("build" | "check" | "emit-ir" | "run"))
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
                bin,
                output: parsed.output,
            }
        }
        Some("check") => {
            let parsed = parse_compile_args("check", &args[1..], false, true)?;
            Action::Check {
                input: parsed.input,
                target: parsed.target,
            }
        }
        Some("emit-ir") => {
            let parsed = parse_compile_args("emit-ir", &args[1..], true, true)?;
            Action::EmitIr {
                input: parsed.input,
                target: parsed.target,
                output: parsed.output,
            }
        }
        Some("run") => parse_run(&args[1..])?,
        _ if starts_with_dash(first)
            && !matches!(first.to_str(), Some("-o" | "--output" | "--bin")) =>
        {
            return Err(format!("unknown option '{}'", first.to_string_lossy()));
        }
        _ => {
            let parsed = parse_compile_args("build", &args, true, false)?;
            let bin = selection_bin(parsed.target, "build")?;
            Action::Build {
                input: parsed.input,
                bin,
                output: parsed.output,
            }
        }
    };

    Ok(ParsedArgs::Action(action))
}

fn parse_compile_args(
    command: &str,
    args: &[OsString],
    allow_output: bool,
    allow_lib: bool,
) -> Result<CompileArgs, String> {
    let mut input = None;
    let mut output = None;
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
        bin,
        args: program_args,
    })
}

fn execute(action: Action) -> i32 {
    match action {
        Action::Build { input, bin, output } => {
            let target = match resolve_input(
                input.as_deref(),
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
        Action::Check { input, target } => {
            let target = match resolve_input(input.as_deref(), target, false) {
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
            target,
            output,
        } => {
            let target = match resolve_input(input.as_deref(), target, false) {
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
        Action::Run { input, bin, args } => {
            let target = match resolve_input(
                input.as_deref(),
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
    }
}

struct ResolvedTarget {
    source: PathBuf,
    project: Option<ProjectTarget>,
    is_library: bool,
    protected_inputs: Vec<PathBuf>,
}

struct ProjectTarget {
    package_root: PathBuf,
    target_name: String,
    packages: Vec<ResolvedPackage>,
}

struct ResolvedPackage {
    id: PackageId,
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
    selection: TargetSelection,
    binary_only: bool,
) -> Result<ResolvedTarget, String> {
    if let Some(path) = input {
        if path.extension() == Some(OsStr::new("sali")) {
            if !matches!(selection, TargetSelection::Default) {
                return Err("target selectors require a salicin.toml package input".into());
            }
            return Ok(ResolvedTarget {
                source: path.to_path_buf(),
                project: None,
                is_library: false,
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
                "input '{}' must be a .sali file, a package directory, or {MANIFEST_FILE_NAME}",
                path.display()
            ));
        }
        None => find_manifest_from_current_dir()?,
    };

    let graph = load_dependency_graph(&manifest_path).map_err(|error| error.to_string())?;
    let selected = select_manifest_target(graph.root(), selection, binary_only)?;
    let packages = resolve_project_packages(&graph, &selected)?;
    write_package_lockfile(&graph).map_err(|error| {
        format!(
            "could not write lockfile '{}': {error}",
            graph.root().package_root.join(LOCKFILE_NAME).display()
        )
    })?;

    let mut protected_inputs = Vec::new();
    for manifest in &graph.packages {
        protected_inputs.push(manifest.manifest_path.clone());
        protected_inputs.extend(manifest.targets().map(|target| target.path.clone()));
    }
    for package in &packages {
        protected_inputs.push(package.source.clone());
        protected_inputs.extend(package.module_sources.iter().map(|(path, _)| path.clone()));
    }
    protected_inputs.push(graph.root().package_root.join(LOCKFILE_NAME));

    Ok(ResolvedTarget {
        source: selected.path,
        project: Some(ProjectTarget {
            package_root: graph.root().package_root.clone(),
            target_name: selected.name,
            packages,
        }),
        is_library: selected.kind == TargetKind::Lib,
        protected_inputs,
    })
}

fn resolve_project_packages(
    graph: &DependencyGraph,
    selected: &Target,
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
            is_primary,
            dependencies,
            source: root_source,
            module_sources,
        });
    }
    Ok(packages)
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
    collect_sali_files(&source_root, &mut candidates)?;
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
        if relative == Path::new("main.sali")
            || relative == Path::new("lib.sali")
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

fn collect_sali_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
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
            collect_sali_files(&entry.path(), output)?;
        } else if file_type.is_file() && entry.path().extension() == Some(OsStr::new("sali")) {
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

fn check_file(source: &Path, library: bool) -> Result<(), ()> {
    if !library {
        return compile_file(source, false).map(|_| ());
    }

    let text = read_source(source)?;
    report_compilation(source, check_library_source(&text))
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
                    eprintln!("{}: {diagnostic}", source.display());
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

    let status = Command::new(compiler)
        .arg("-Wno-override-module")
        .arg("-x")
        .arg("ir")
        .arg(ir)
        .arg("-x")
        .arg("c")
        .arg("-std=c11")
        .arg(runtime)
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
