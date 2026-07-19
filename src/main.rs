use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command, ExitStatus};
use std::time::{SystemTime, UNIX_EPOCH};

use salicin_lang::compile_source;

const HELP: &str = "Salicin compiler

Usage:
  salic build <file.sali> [-o <path>]
  salic check <file.sali>
  salic emit-ir <file.sali> [-o <path>]
  salic run <file.sali> [-- <args>...]
  salic <file.sali> [-o <path>]

Commands:
  build      Compile a Salicin source file to a native executable
  check      Parse and type-check a source file without writing output
  emit-ir    Print LLVM IR, or write it to a file with -o
  run        Compile and run a program; arguments after -- go to the program

Options:
  -o, --output <path>  Select the output path
  -h, --help           Print this help
  -V, --version        Print the compiler version

Without an explicit command, a .sali input is treated as a build command.
The default build output is the source path without its .sali extension.";

enum ParsedArgs {
    Help,
    Version,
    Action(Action),
}

enum Action {
    Build {
        source: PathBuf,
        output: Option<PathBuf>,
    },
    Check {
        source: PathBuf,
    },
    EmitIr {
        source: PathBuf,
        output: Option<PathBuf>,
    },
    Run {
        source: PathBuf,
        args: Vec<OsString>,
    },
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
        return Err("missing a command or .sali source file".into());
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
            let (source, output) = parse_source_and_output("build", &args[1..])?;
            Action::Build { source, output }
        }
        Some("check") => Action::Check {
            source: parse_single_source("check", &args[1..])?,
        },
        Some("emit-ir") => {
            let (source, output) = parse_source_and_output("emit-ir", &args[1..])?;
            Action::EmitIr { source, output }
        }
        Some("run") => parse_run(&args[1..])?,
        _ if starts_with_dash(first) => {
            return Err(format!("unknown option '{}'", first.to_string_lossy()));
        }
        _ => {
            let (source, output) = parse_source_and_output("build", &args)?;
            Action::Build { source, output }
        }
    };

    Ok(ParsedArgs::Action(action))
}

fn parse_source_and_output(
    command: &str,
    args: &[OsString],
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let mut source = None;
    let mut output = None;
    let mut index = 0;

    while index < args.len() {
        let argument = &args[index];
        if is(argument, "-o") || is(argument, "--output") {
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
        } else if starts_with_dash(argument) {
            return Err(format!(
                "unknown option '{}' for '{command}'",
                argument.to_string_lossy()
            ));
        } else if source.is_some() {
            return Err(format!(
                "'{command}' accepts exactly one source file; unexpected argument '{}'",
                argument.to_string_lossy()
            ));
        } else {
            source = Some(PathBuf::from(argument));
        }
        index += 1;
    }

    let source = source.ok_or_else(|| format!("'{command}' requires a .sali source file"))?;
    validate_source_path(&source)?;
    Ok((source, output))
}

fn parse_single_source(command: &str, args: &[OsString]) -> Result<PathBuf, String> {
    if args.len() != 1 {
        return Err(format!(
            "'{command}' requires exactly one .sali source file"
        ));
    }
    if starts_with_dash(&args[0]) {
        return Err(format!(
            "unknown option '{}' for '{command}'",
            args[0].to_string_lossy()
        ));
    }

    let source = PathBuf::from(&args[0]);
    validate_source_path(&source)?;
    Ok(source)
}

fn parse_run(args: &[OsString]) -> Result<Action, String> {
    let separator = args.iter().position(|argument| is(argument, "--"));
    let (compiler_args, program_args) = match separator {
        Some(index) => (&args[..index], args[index + 1..].to_vec()),
        None => (args, Vec::new()),
    };

    if compiler_args.len() != 1 {
        let hint = if separator.is_none() && compiler_args.len() > 1 {
            "; place program arguments after '--'"
        } else {
            ""
        };
        return Err(format!(
            "'run' requires exactly one .sali source file{hint}"
        ));
    }
    if starts_with_dash(&compiler_args[0]) {
        return Err(format!(
            "unknown option '{}' for 'run'",
            compiler_args[0].to_string_lossy()
        ));
    }

    let source = PathBuf::from(&compiler_args[0]);
    validate_source_path(&source)?;
    Ok(Action::Run {
        source,
        args: program_args,
    })
}

fn validate_source_path(source: &Path) -> Result<(), String> {
    if source.extension() != Some(OsStr::new("sali")) {
        return Err(format!(
            "source file '{}' must use the .sali extension",
            source.display()
        ));
    }
    Ok(())
}

fn execute(action: Action) -> i32 {
    match action {
        Action::Build { source, output } => {
            let output = match output {
                Some(output) => output,
                None => match default_output_path(&source) {
                    Ok(output) => output,
                    Err(message) => {
                        eprintln!("salic: {message}");
                        return 2;
                    }
                },
            };
            if let Err(message) = ensure_distinct_output(&source, &output) {
                eprintln!("salic: {message}");
                return 2;
            }

            let ir = match compile_file(&source) {
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
        Action::Check { source } => match compile_file(&source) {
            Ok(_) => 0,
            Err(()) => 1,
        },
        Action::EmitIr { source, output } => {
            if let Some(path) = output.as_deref() {
                if path != Path::new("-") {
                    if let Err(message) = ensure_distinct_output(&source, path) {
                        eprintln!("salic: {message}");
                        return 2;
                    }
                }
            }
            let ir = match compile_file(&source) {
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
        Action::Run { source, args } => {
            let ir = match compile_file(&source) {
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

fn ensure_distinct_output(source: &Path, output: &Path) -> Result<(), String> {
    if paths_refer_to_same_file(source, output) {
        return Err(format!(
            "refusing to overwrite source file '{}'",
            source.display()
        ));
    }
    Ok(())
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

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if let (Ok(left), Ok(right)) = (fs::metadata(left), fs::metadata(right)) {
            return left.dev() == right.dev() && left.ino() == right.ino();
        }
    }

    false
}

fn compile_file(source: &Path) -> Result<String, ()> {
    let text = match fs::read_to_string(source) {
        Ok(text) => text,
        Err(error) => {
            eprintln!(
                "salic: could not read source file '{}': {error}",
                source.display()
            );
            return Err(());
        }
    };

    match compile_source(&text) {
        Ok(ir) => Ok(ir),
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
    fs::write(&ir_path, ir).map_err(|error| {
        format!(
            "could not write temporary LLVM IR '{}': {error}",
            ir_path.display()
        )
    })?;
    invoke_clang(&ir_path, output)
}

fn compile_and_run(ir: &str, args: &[OsString]) -> Result<i32, String> {
    let temporary = TemporaryDirectory::new()?;
    let ir_path = temporary.path().join("module.ll");
    let executable = temporary.path().join(executable_name("program"));

    fs::write(&ir_path, ir).map_err(|error| {
        format!(
            "could not write temporary LLVM IR '{}': {error}",
            ir_path.display()
        )
    })?;
    invoke_clang(&ir_path, &executable)?;

    let status = Command::new(&executable)
        .args(args)
        .status()
        .map_err(|error| format!("could not run '{}': {error}", executable.display()))?;
    Ok(program_exit_code(status))
}

fn invoke_clang(ir: &Path, output: &Path) -> Result<(), String> {
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
