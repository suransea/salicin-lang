pub(crate) use std::collections::BTreeMap;
pub(crate) use std::fs;
use std::os::unix::process::ExitStatusExt;
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) use salicin_lang::modules::{PackageId, SourcePackage, SourceUnit};
pub(crate) use salicin_lang::{
    check_library_source, check_source, check_source_packages, compile_library_source_packages,
    compile_source, compile_test_source_packages, formatter::format_source,
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
static TEST_ALLOCATOR_OBJECT: OnceLock<PathBuf> = OnceLock::new();
const TEST_ALLOCATOR_RUNTIME: &str = include_str!("../../runtime/allocator.c");

pub(crate) fn salic() -> Command {
    Command::new(env!("CARGO_BIN_EXE_salic"))
}

pub(crate) fn fixture(kind: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(kind)
        .join(name)
}

/// Returns every Salicin fixture below a corpus directory in stable order.
///
/// Discovery is recursive so fixtures can be grouped by language feature
/// without changing the corpus tests.
pub(crate) fn fixture_paths(kind: &str) -> Vec<PathBuf> {
    fn collect(directory: &Path, paths: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).unwrap_or_else(|error| {
            panic!("read fixture directory '{}': {error}", directory.display())
        }) {
            let path = entry.expect("read fixture entry").path();
            if path.is_dir() {
                collect(&path, paths);
            } else if path.extension().is_some_and(|extension| extension == "sc") {
                paths.push(path);
            }
        }
    }

    let directory = fixture(kind, "");
    let mut paths = Vec::new();
    collect(&directory, &mut paths);
    paths.sort();
    paths
}

pub(crate) fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

pub(crate) fn test_allocator_object() -> &'static Path {
    TEST_ALLOCATOR_OBJECT.get_or_init(|| {
        let source =
            std::env::temp_dir().join(format!("salic-test-allocator-{}.c", std::process::id()));
        let object = source.with_extension("o");
        fs::write(&source, TEST_ALLOCATOR_RUNTIME).expect("write test allocator runtime");
        let output = Command::new("/usr/bin/clang")
            .arg("-c")
            .arg("-x")
            .arg("c")
            .arg("-std=c11")
            .arg(&source)
            .arg("-o")
            .arg(&object)
            .output()
            .expect("compile test allocator runtime");
        assert!(output.status.success(), "{}", output_text(&output));
        let _ = fs::remove_file(source);
        object
    })
}

pub(crate) fn link_and_run_ir(ir: &str, description: &str) -> Output {
    let temporary = TestDirectory::new();
    let ir_path = temporary.write("module.ll", ir);
    let executable = temporary.join("program");
    let linked = Command::new("/usr/bin/clang")
        .arg("-Wno-override-module")
        .arg("-x")
        .arg("ir")
        .arg(&ir_path)
        .arg("-x")
        .arg("none")
        .arg(test_allocator_object())
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap_or_else(|error| panic!("link {description}: {error}"));
    assert!(
        linked.status.success(),
        "{description}: {}",
        output_text(&linked)
    );
    Command::new(executable)
        .output()
        .unwrap_or_else(|error| panic!("run {description}: {error}"))
}

pub(crate) fn link_and_run_ir_with_c(ir: &str, c_source: &str, description: &str) -> Output {
    let temporary = TestDirectory::new();
    let ir_path = temporary.write("module.ll", ir);
    let c_path = temporary.write("interop.c", c_source);
    let executable = temporary.join("program");
    let linked = Command::new("/usr/bin/clang")
        .arg("-Wno-override-module")
        .arg("-std=c11")
        .arg("-x")
        .arg("ir")
        .arg(&ir_path)
        .arg("-x")
        .arg("c")
        .arg(&c_path)
        .arg("-x")
        .arg("none")
        .arg(test_allocator_object())
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap_or_else(|error| panic!("link {description}: {error}"));
    assert!(
        linked.status.success(),
        "{description}: {}",
        output_text(&linked)
    );
    Command::new(executable)
        .output()
        .unwrap_or_else(|error| panic!("run {description}: {error}"))
}

fn run_source_in_process(path: &Path) -> Output {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read '{}': {error}", path.display()));
    let ir = compile_source(&source).unwrap_or_else(|diagnostics| {
        panic!("compile '{}':\n{}", path.display(), diagnostics.join("\n"))
    });
    link_and_run_ir(&ir, &format!("fixture '{}'", path.display()))
}

pub(crate) fn trapping_fixture_outputs_in_parallel(names: &[&str]) -> Vec<(String, Output)> {
    let paths = names
        .iter()
        .map(|name| fixture("pass", name))
        .collect::<Vec<_>>();
    let jobs = Arc::new(Mutex::new(paths.into_iter().enumerate()));
    let results = Arc::new(Mutex::new(Vec::new()));
    thread::scope(|scope| {
        let jobs = Arc::clone(&jobs);
        let results = Arc::clone(&results);
        thread::Builder::new()
            .name("salic-trapping-fixture".into())
            .stack_size(16 * 1024 * 1024)
            .spawn_scoped(scope, move || loop {
                let Some((index, path)) = jobs.lock().expect("lock native jobs").next() else {
                    break;
                };
                let output = run_source_in_process(&path);
                results
                    .lock()
                    .expect("lock native results")
                    .push((index, path, output));
            })
            .expect("spawn trapping-fixture worker");
    });
    let mut results = Arc::try_unwrap(results)
        .expect("native workers released results")
        .into_inner()
        .expect("unlock native results");
    results.sort_by_key(|(index, _, _)| *index);
    results
        .into_iter()
        .map(|(_, path, output)| {
            (
                path.file_name()
                    .expect("fixture path has a file name")
                    .to_string_lossy()
                    .into_owned(),
                output,
            )
        })
        .collect()
}

pub(crate) fn batched_native_fixture_outputs(names: &[&str]) -> Vec<(String, Output)> {
    const MODULE_INCOMPATIBLE: &[&str] = &["match_literal_payload.sc"];
    let batch_names = names
        .iter()
        .copied()
        .filter(|name| !MODULE_INCOMPATIBLE.contains(name))
        .collect::<Vec<_>>();
    let mut sources = vec![SourceUnit {
        path: "<native-fixture-root>".into(),
        module_path: Vec::new(),
        source: String::new(),
        is_root: true,
    }];
    for name in &batch_names {
        let path = fixture("pass", name);
        let module = path
            .file_stem()
            .expect("fixture path has a stem")
            .to_string_lossy()
            .into_owned();
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read '{}': {error}", path.display()));
        sources.push(SourceUnit {
            path: path.display().to_string(),
            module_path: vec![module],
            source,
            is_root: false,
        });
    }
    let batch = if batch_names.is_empty() {
        None
    } else {
        let compilation = thread::Builder::new()
            .name("salic-native-fixture-compile".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                compile_test_source_packages(&[SourcePackage {
                    id: PackageId(0),
                    name: "fixtures".to_owned(),
                    version: "0.0.0".to_owned(),
                    identity: "fixtures@0.0.0".to_owned(),
                    is_primary: true,
                    dependencies: BTreeMap::new(),
                    sources,
                }])
            })
            .expect("spawn native fixture compiler")
            .join()
            .expect("native fixture compiler panicked")
            .unwrap_or_else(|diagnostics| {
                panic!(
                    "compile batched native fixtures:\n{}",
                    diagnostics.join("\n")
                )
            });
        assert_eq!(
            compilation.names, batch_names,
            "native fixtures must declare exactly one matching test each"
        );
        let mut output = link_and_run_ir(&compilation.ir, "batched native fixtures");
        if output.status.success() {
            output.status = std::process::ExitStatus::from_raw(42 << 8);
        } else if let Some(index) = output.status.code().and_then(|index| index.checked_sub(1)) {
            if let Some(name) = compilation.names.get(index as usize) {
                output.stderr.extend_from_slice(
                    format!("batched native fixture {name:?} returned a value other than 42\n")
                        .as_bytes(),
                );
            }
        }
        Some(output)
    };
    names
        .iter()
        .map(|name| {
            let output = if MODULE_INCOMPATIBLE.contains(name) {
                run_source_in_process(&fixture("pass", name))
            } else {
                batch
                    .as_ref()
                    .expect("a compatible fixture has a batch output")
                    .clone()
            };
            ((*name).to_owned(), output)
        })
        .collect()
}

pub(crate) fn check_sources_in_parallel(
    paths: Vec<PathBuf>,
) -> Vec<(PathBuf, Result<(), Vec<String>>)> {
    if paths.is_empty() {
        return Vec::new();
    }

    let available = thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(2);
    let worker_count = available.clamp(1, 8).min(paths.len());
    let jobs = Arc::new(Mutex::new(paths.into_iter().enumerate()));
    let results = Arc::new(Mutex::new(Vec::new()));

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let jobs = Arc::clone(&jobs);
            let results = Arc::clone(&results);
            thread::Builder::new()
                .name("salic-fixture-check".into())
                .stack_size(16 * 1024 * 1024)
                .spawn_scoped(scope, move || loop {
                    let Some((index, path)) = jobs.lock().expect("lock source jobs").next() else {
                        break;
                    };
                    let source = fs::read_to_string(&path)
                        .unwrap_or_else(|error| panic!("read '{}': {error}", path.display()));
                    let checked = check_source(&source);
                    results
                        .lock()
                        .expect("lock source results")
                        .push((index, path, checked));
                })
                .expect("spawn source-check worker");
        }
    });

    let mut results = Arc::try_unwrap(results)
        .expect("source workers released results")
        .into_inner()
        .expect("unlock source results");
    results.sort_by_key(|(index, _, _)| *index);
    results
        .into_iter()
        .map(|(_, path, checked)| (path, checked))
        .collect()
}

/// Checks the passing corpus as one package, sharing core parsing, module
/// resolution, and semantic setup across all fixtures.
pub(crate) fn check_passing_fixture_corpus() -> Result<(), Vec<String>> {
    const BATCH_SIZE: usize = 48;
    // This fixture intentionally exercises a root-only literal payload form.
    const ROOT_ONLY: &[&str] = &["match_literal_payload.sc"];

    let paths = fixture_paths("pass");
    let mut fixture_sources = Vec::new();
    let mut root_only = Vec::new();

    for (index, path) in paths.into_iter().enumerate() {
        if path
            .file_name()
            .is_some_and(|name| ROOT_ONLY.iter().any(|candidate| name == *candidate))
        {
            root_only.push(path);
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read '{}': {error}", path.display()));
        fixture_sources.push(SourceUnit {
            path: path.display().to_string(),
            module_path: vec![format!("fixture_{index}")],
            source,
            is_root: false,
        });
    }

    let batches = fixture_sources
        .chunks(BATCH_SIZE)
        .enumerate()
        .map(|(index, fixtures)| {
            let mut sources = vec![SourceUnit {
                path: format!("<pass-fixture-root-{index}>"),
                module_path: Vec::new(),
                source: "let main(): i32 = { 42 }\n".to_owned(),
                is_root: true,
            }];
            sources.extend_from_slice(fixtures);
            (index, sources)
        })
        .collect::<Vec<_>>();
    let worker_count = thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(2)
        .clamp(1, 8)
        .min(batches.len());
    let jobs = Arc::new(Mutex::new(batches.into_iter()));
    let errors = Arc::new(Mutex::new(Vec::new()));
    thread::scope(|scope| {
        for _ in 0..worker_count {
            let jobs = Arc::clone(&jobs);
            let errors = Arc::clone(&errors);
            thread::Builder::new()
                .name("salic-pass-corpus-check".into())
                .stack_size(16 * 1024 * 1024)
                .spawn_scoped(scope, move || loop {
                    let Some((index, sources)) = jobs.lock().expect("lock pass corpus jobs").next()
                    else {
                        break;
                    };
                    let name = format!("pass-fixtures-{index}");
                    if let Err(diagnostics) = check_source_packages(&[SourcePackage {
                        id: PackageId(index),
                        name: name.clone(),
                        version: "0.0.0".to_owned(),
                        identity: format!("{name}@0.0.0"),
                        is_primary: true,
                        dependencies: BTreeMap::new(),
                        sources,
                    }]) {
                        errors
                            .lock()
                            .expect("lock pass corpus errors")
                            .extend(diagnostics);
                    }
                })
                .expect("spawn passing corpus checker");
        }
    });

    let errors = Arc::try_unwrap(errors)
        .expect("pass corpus workers released errors")
        .into_inner()
        .expect("unlock pass corpus errors");
    if !errors.is_empty() {
        return Err(errors);
    }

    for path in root_only {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read '{}': {error}", path.display()));
        check_source(&source).map_err(|diagnostics| {
            diagnostics
                .into_iter()
                .map(|diagnostic| format!("{}: {diagnostic}", path.display()))
                .collect::<Vec<_>>()
        })?;
    }
    Ok(())
}

pub(crate) struct TestDirectory(pub(crate) PathBuf);

impl TestDirectory {
    pub(crate) fn new() -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "salic-test-{}-{timestamp}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create test directory");
        Self(path)
    }

    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    pub(crate) fn create_dir(&self, name: &str) -> PathBuf {
        let path = self.join(name);
        fs::create_dir_all(&path).expect("create nested test directory");
        path
    }

    pub(crate) fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create test fixture parent directory");
        }
        fs::write(&path, contents).expect("write test fixture");
        path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
