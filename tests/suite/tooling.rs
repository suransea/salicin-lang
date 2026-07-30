use crate::support::*;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};
use std::process::Stdio;

fn digest_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn lsp_stdio_selects_a_workspace_package_and_synchronizes_without_writing() {
    let workspace = TestDirectory::new();
    workspace.write(
        "salicin.toml",
        "[workspace]\nmembers = [\"app\", \"other\"]\n",
    );
    workspace.write(
        "app/salicin.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    let source = workspace.write("app/src/main.sc", "let main(): i32 = { 0 }\n");
    workspace.write(
        "other/salicin.toml",
        "[package]\nname = \"other\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    workspace.write("other/src/main.sc", "let main(): i32 = { 1 }\n");

    fn message(value: serde_json::Value) -> Vec<u8> {
        let body = serde_json::to_vec(&value).unwrap();
        format!("Content-Length: {}\r\n\r\n", body.len())
            .into_bytes()
            .into_iter()
            .chain(body)
            .collect()
    }

    let canonical_source = fs::canonicalize(&source).unwrap();
    let uri = salicin_lang::lsp::path_to_file_uri(&canonical_source.display().to_string());
    let mut transcript = message(serde_json::json!({
        "jsonrpc":"2.0", "id":1, "method":"initialize", "params":{}
    }));
    for value in [
        serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
        serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"salicin","version":1,
                "text":"let main(: i32 = { \"😀\" }\n"}
        }}),
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/semanticTokens/full",
            "params":{"textDocument":{"uri":uri}}}),
        serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri,"version":2},
            "contentChanges":[{"text":"let main(): i32 = { 42 }\n"}]
        }}),
        serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{
            "textDocument":{"uri":uri}
        }}),
        serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
            "textDocument":{"uri":uri}
        }}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
        serde_json::json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        transcript.extend(message(value));
    }

    let mut child = salic()
        .args(["lsp", "-p", "app"])
        .arg(&workspace.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start language server");
    child.stdin.take().unwrap().write_all(&transcript).unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", output_text(&output));
    assert!(output.stderr.is_empty(), "{}", output_text(&output));
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "let main(): i32 = { 0 }\n",
        "LSP synchronization must not write source files"
    );

    let mut responses = Cursor::new(output.stdout);
    let mut messages = Vec::new();
    while let Some(message) = salicin_lang::lsp::read_message(&mut responses).unwrap() {
        messages.push(serde_json::from_slice::<serde_json::Value>(&message).unwrap());
    }
    let initialize = messages
        .iter()
        .find(|message| message["id"] == 1)
        .expect("initialize response");
    assert_eq!(initialize["id"], 1);
    assert_eq!(
        initialize["result"]["capabilities"]["textDocumentSync"]["change"],
        1
    );
    assert_eq!(
        initialize["result"]["capabilities"]["semanticTokensProvider"]["full"],
        true
    );
    let shutdown = messages
        .iter()
        .find(|message| message["id"] == 2)
        .expect("shutdown response");
    assert_eq!(shutdown["id"], 2, "unexpected LSP response: {shutdown}");
    let semantic = messages
        .iter()
        .find(|message| message["id"] == 3)
        .expect("semantic token response");
    assert!(!semantic["result"]["data"].as_array().unwrap().is_empty());
    let published = messages
        .iter()
        .filter(|message| {
            message["method"] == "textDocument/publishDiagnostics"
                && message["params"]["uri"] == uri
        })
        .collect::<Vec<_>>();
    assert_eq!(published.len(), 5);
    assert!(published[0]["params"]["diagnostics"]
        .as_array()
        .is_some_and(Vec::is_empty));
    assert_eq!(
        published[1]["params"]["diagnostics"][0]["data"]["phase"],
        "parser"
    );
    assert!(published[2..]
        .iter()
        .all(|message| message["params"]["diagnostics"]
            .as_array()
            .is_some_and(Vec::is_empty)));
    assert_eq!(published[1]["params"]["version"], 1);
    assert_eq!(published[2]["params"]["version"], 2);
    assert!(published[4]["params"].get("version").is_none());
}

#[test]
fn emit_ir_build_and_run_reuse_the_same_validated_binary_cache_entry() {
    let temporary = TestDirectory::new();
    let source = temporary.write("main.sc", "let main(): i32 = { 42 }\n");
    let cache = temporary.create_dir("cache");
    let first_ir = temporary.join("first.ll");
    let second_ir = temporary.join("second.ll");
    let first = salic()
        .arg("emit-ir")
        .arg(&source)
        .arg("-o")
        .arg(&first_ir)
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .expect("populate cache");
    assert!(first.status.success(), "{}", output_text(&first));

    let fingerprint = salic().arg("fingerprint").arg(&source).output().unwrap();
    assert!(
        fingerprint.status.success(),
        "{}",
        output_text(&fingerprint)
    );
    let fingerprint = String::from_utf8(fingerprint.stdout).unwrap();
    let entry = cache_entry(&cache, fingerprint.trim());
    let payload_path = entry.join("module.ll");
    let metadata_path = entry.join("metadata.toml");
    let old_payload = fs::read_to_string(&payload_path).unwrap();
    let new_payload = format!("; warm-cache-sentinel\n{old_payload}");
    fs::write(&payload_path, &new_payload).unwrap();
    let metadata = fs::read_to_string(&metadata_path).unwrap();
    let metadata = replace_metadata_field(
        metadata,
        "payload_bytes",
        &old_payload.len().to_string(),
        &new_payload.len().to_string(),
    );
    let metadata = replace_metadata_field(
        metadata,
        "payload_sha256",
        &format!("\"{}\"", digest_hex(old_payload.as_bytes())),
        &format!("\"{}\"", digest_hex(new_payload.as_bytes())),
    );
    fs::write(&metadata_path, metadata).unwrap();

    let second = salic()
        .arg("emit-ir")
        .arg(&source)
        .arg("-o")
        .arg(&second_ir)
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .expect("reuse cache");
    assert!(second.status.success(), "{}", output_text(&second));
    assert!(fs::read_to_string(&second_ir)
        .unwrap()
        .starts_with("; warm-cache-sentinel\n"));
    let executable = temporary.join("cached-program");
    let build = salic()
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert!(build.status.success(), "{}", output_text(&build));
    let run = salic()
        .arg("run")
        .arg(&source)
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(42), "{}", output_text(&run));
}

#[test]
fn test_list_reuses_cached_ordered_test_names() {
    let temporary = TestDirectory::new();
    let source = temporary.write("tests.sc", "test(\"alpha\") { }\n");
    let cache = temporary.create_dir("cache");
    let first = salic()
        .arg("test")
        .arg(&source)
        .arg("--list")
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert!(first.status.success(), "{}", output_text(&first));
    assert_eq!(String::from_utf8_lossy(&first.stdout), "alpha\n");

    let packages = vec![SourcePackage {
        id: PackageId(0),
        name: "source".into(),
        version: "0.0.0".into(),
        identity: "source@0.0.0".into(),
        is_primary: true,
        dependencies: BTreeMap::new(),
        sources: vec![SourceUnit {
            path: source.display().to_string(),
            module_path: Vec::new(),
            source: fs::read_to_string(&source).unwrap(),
            is_root: true,
        }],
    }];
    let fingerprint = salicin_lang::incremental::fingerprint_package_graph(
        &packages,
        salicin_lang::manifest::Edition::Edition2026,
        salicin_lang::incremental::IncrementalTarget::Test { filter: None },
    )
    .unwrap()
    .to_hex();
    let metadata_path = cache_entry(&cache, &fingerprint).join("metadata.toml");
    let metadata = fs::read_to_string(&metadata_path).unwrap();
    let metadata =
        replace_metadata_field(metadata, "test_names", "[\"alpha\"]", "[\"cached-alpha\"]");
    let metadata = replace_metadata_field(
        metadata,
        "test_names_sha256",
        &format!("\"{}\"", test_names_digest(&["alpha"])),
        &format!("\"{}\"", test_names_digest(&["cached-alpha"])),
    );
    fs::write(&metadata_path, metadata).unwrap();
    let second = salic()
        .arg("test")
        .arg(&source)
        .arg("--list")
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert!(second.status.success(), "{}", output_text(&second));
    assert_eq!(String::from_utf8_lossy(&second.stdout), "cached-alpha\n");
}

#[test]
fn cache_trace_reports_cold_warm_and_bypassed_decisions_only_on_stderr() {
    let temporary = TestDirectory::new();
    let source = temporary.write("main.sc", "let main(): i32 = { 0 }\n");
    let cache = temporary.create_dir("cache");

    let invoke = || {
        salic()
            .arg("emit-ir")
            .arg(&source)
            .args(["--cache-trace", "-o", "-"])
            .env("SALICIN_CACHE_DIR", &cache)
            .output()
            .unwrap()
    };
    let cold = invoke();
    assert!(cold.status.success(), "{}", output_text(&cold));
    let cold_stderr = String::from_utf8_lossy(&cold.stderr);
    assert!(cold_stderr.contains(": miss (missing)"), "{cold_stderr}");
    assert!(cold_stderr.contains(": published"), "{cold_stderr}");
    assert!(String::from_utf8_lossy(&cold.stdout).contains("define i32 @main"));

    let warm = invoke();
    assert!(warm.status.success(), "{}", output_text(&warm));
    let warm_stderr = String::from_utf8_lossy(&warm.stderr);
    assert!(warm_stderr.contains(": hit"), "{warm_stderr}");
    assert!(!String::from_utf8_lossy(&warm.stdout).contains("salic: cache"));

    let fingerprint = salic().arg("fingerprint").arg(&source).output().unwrap();
    let fingerprint = String::from_utf8(fingerprint.stdout).unwrap();
    fs::write(
        cache_entry(&cache, fingerprint.trim()).join("module.ll"),
        "damaged",
    )
    .unwrap();
    let repaired = invoke();
    assert!(repaired.status.success(), "{}", output_text(&repaired));
    let repaired_stderr = String::from_utf8_lossy(&repaired.stderr);
    assert!(
        repaired_stderr.contains(": miss (invalid-payload)"),
        "{repaired_stderr}"
    );
    assert!(repaired_stderr.contains(": published"), "{repaired_stderr}");

    let bypassed = salic()
        .arg("emit-ir")
        .arg(&source)
        .args(["--no-cache", "--cache-trace", "-o", "-"])
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert!(bypassed.status.success(), "{}", output_text(&bypassed));
    let bypassed_stderr = String::from_utf8_lossy(&bypassed.stderr);
    assert!(bypassed_stderr.contains(": bypassed"), "{bypassed_stderr}");
    assert!(!bypassed_stderr.contains(": hit"), "{bypassed_stderr}");
    assert!(
        !bypassed_stderr.contains(": published"),
        "{bypassed_stderr}"
    );

    let fresh_cache = temporary.create_dir("fresh-cache");
    let fresh_bypass = salic()
        .arg("emit-ir")
        .arg(&source)
        .args(["--no-cache", "-o", "-"])
        .env("SALICIN_CACHE_DIR", &fresh_cache)
        .output()
        .unwrap();
    assert!(
        fresh_bypass.status.success(),
        "{}",
        output_text(&fresh_bypass)
    );
    assert!(
        fs::read_dir(&fresh_cache).unwrap().next().is_none(),
        "bypass must not initialize or publish into the cache root"
    );

    let disabled = salic()
        .arg("emit-ir")
        .arg(&source)
        .args(["--cache-trace", "-o", "-"])
        .env("SALICIN_CACHE_DIR", "relative-cache")
        .current_dir(&temporary.0)
        .output()
        .unwrap();
    assert!(disabled.status.success(), "{}", output_text(&disabled));
    assert!(
        String::from_utf8_lossy(&disabled.stderr).contains(": disabled (relative-override)"),
        "{}",
        output_text(&disabled)
    );
    assert!(!temporary.join("relative-cache").exists());
}

#[test]
fn cache_clean_preserves_unowned_root_data_and_refuses_unowned_roots() {
    let temporary = TestDirectory::new();
    let source = temporary.write("main.sc", "let main(): i32 = { 0 }\n");
    let cache = temporary.create_dir("cache");
    let populated = salic()
        .arg("emit-ir")
        .arg(&source)
        .args(["-o", "-"])
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert!(populated.status.success(), "{}", output_text(&populated));
    fs::write(cache.join("unrelated"), "preserve").unwrap();

    let cleaned = salic()
        .args(["cache", "clean"])
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert!(cleaned.status.success(), "{}", output_text(&cleaned));
    assert!(!cache.join("llvm-ir").exists());
    assert_eq!(
        fs::read_to_string(cache.join("unrelated")).unwrap(),
        "preserve"
    );
    assert!(cache.join(".salicin-cache-root").is_file());

    let empty = salic()
        .args(["cache", "clean"])
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert!(empty.status.success(), "{}", output_text(&empty));
    assert!(String::from_utf8_lossy(&empty.stdout).contains("already empty"));

    let unowned = temporary.create_dir("unowned");
    fs::write(unowned.join("user-data"), "keep").unwrap();
    let refused = salic()
        .args(["cache", "clean"])
        .env("SALICIN_CACHE_DIR", &unowned)
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1), "{}", output_text(&refused));
    assert_eq!(
        fs::read_to_string(unowned.join("user-data")).unwrap(),
        "keep"
    );
}

#[test]
fn cache_reuses_byte_identical_ir_across_checkout_relocation_and_command_targets() {
    fn write_project(project: &TestDirectory) {
        project.write(
            "salicin.toml",
            "[package]\nname = \"relocatable\"\nversion = \"1.0.0\"\nedition = \"2026\"\n",
        );
        project.write(
            "src/main.sc",
            "let main(): i32 = { shared.answer() }\ntest(\"answer\") { () }\n",
        );
        project.write(
            "src/lib.sc",
            "pub let answer(): i32 = { shared.answer() }\n",
        );
        project.write("src/shared.sc", "pub(package) let answer(): i32 = { 42 }\n");
    }
    let first = TestDirectory::new();
    let relocated = TestDirectory::new();
    let cache_owner = TestDirectory::new();
    let cache = cache_owner.create_dir("cache");
    write_project(&first);
    write_project(&relocated);

    let emit = |project: &TestDirectory, extra: &[&str]| {
        let mut command = salic();
        command
            .arg("emit-ir")
            .arg(&project.0)
            .args(extra)
            .args(["--cache-trace", "-o", "-"])
            .env("SALICIN_CACHE_DIR", &cache);
        command.output().unwrap()
    };
    let cold = emit(&first, &[]);
    let warm = emit(&relocated, &[]);
    assert!(cold.status.success(), "{}", output_text(&cold));
    assert!(warm.status.success(), "{}", output_text(&warm));
    assert_eq!(cold.stdout, warm.stdout);
    assert!(String::from_utf8_lossy(&cold.stderr).contains(": miss (missing)"));
    assert!(String::from_utf8_lossy(&warm.stderr).contains(": hit"));

    let library = emit(&relocated, &["--lib"]);
    assert!(library.status.success(), "{}", output_text(&library));
    assert!(String::from_utf8_lossy(&library.stderr).contains("cache library"));
    assert!(String::from_utf8_lossy(&library.stderr).contains(": miss (missing)"));

    let tests = salic()
        .arg("test")
        .arg(&relocated.0)
        .args(["--list", "--cache-trace"])
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert!(tests.status.success(), "{}", output_text(&tests));
    assert_eq!(String::from_utf8_lossy(&tests.stdout), "answer\n");
    assert!(String::from_utf8_lossy(&tests.stderr).contains("cache test"));
    assert!(String::from_utf8_lossy(&tests.stderr).contains(": miss (missing)"));
}

#[test]
fn cache_rejects_canonical_schema_and_compiler_metadata_changes_end_to_end() {
    let temporary = TestDirectory::new();
    let source = temporary.write("main.sc", "let main(): i32 = { 0 }\n");
    let cache = temporary.create_dir("cache");
    let populate = salic()
        .arg("emit-ir")
        .arg(&source)
        .args(["-o", "-"])
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert!(populate.status.success(), "{}", output_text(&populate));
    let fingerprint = command_fingerprint(&source);
    let metadata_path = cache_entry(&cache, &fingerprint).join("metadata.toml");

    let current_compiler = format!("\"{}\"", env!("CARGO_PKG_VERSION"));
    let cases = [
        ("schema", "2".to_owned(), "3".to_owned()),
        (
            "compiler",
            current_compiler,
            "\"different-compiler\"".to_owned(),
        ),
    ];
    for (field, old, changed) in cases {
        let metadata = fs::read_to_string(&metadata_path).unwrap();
        let metadata = replace_metadata_field(metadata, field, &old, &changed);
        fs::write(&metadata_path, metadata).unwrap();
        let output = salic()
            .arg("emit-ir")
            .arg(&source)
            .args(["--cache-trace", "-o", "-"])
            .env("SALICIN_CACHE_DIR", &cache)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", output_text(&output));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(": miss (incompatible-metadata)"),
            "{stderr}"
        );
        assert!(stderr.contains(": published"), "{stderr}");
    }
}

#[test]
fn failed_compilation_never_publishes_its_fingerprint() {
    let temporary = TestDirectory::new();
    let source = temporary.write("broken.sc", "let main(): i32 = { missing }\n");
    let cache = temporary.create_dir("cache");
    let fingerprint = command_fingerprint(&source);
    let output = salic()
        .arg("emit-ir")
        .arg(&source)
        .args(["--cache-trace", "-o", "-"])
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert!(String::from_utf8_lossy(&output.stderr).contains(": miss (missing)"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(": published"));
    assert!(!cache_entry(&cache, &fingerprint).exists());
}

#[test]
fn concurrent_cache_readers_observe_only_the_complete_warm_artifact() {
    let temporary = TestDirectory::new();
    let source = temporary.write("main.sc", "let main(): i32 = { 42 }\n");
    let cache = temporary.create_dir("cache");
    let cold = salic()
        .arg("emit-ir")
        .arg(&source)
        .args(["-o", "-"])
        .env("SALICIN_CACHE_DIR", &cache)
        .output()
        .unwrap();
    assert!(cold.status.success(), "{}", output_text(&cold));

    let mut readers = Vec::new();
    for _ in 0..8 {
        readers.push(
            salic()
                .arg("emit-ir")
                .arg(&source)
                .args(["--cache-trace", "-o", "-"])
                .env("SALICIN_CACHE_DIR", &cache)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for reader in readers {
        let output = reader.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", output_text(&output));
        assert_eq!(output.stdout, cold.stdout);
        assert!(String::from_utf8_lossy(&output.stderr).contains(": hit"));
    }
}

fn command_fingerprint(source: &Path) -> String {
    let output = salic().arg("fingerprint").arg(source).output().unwrap();
    assert!(output.status.success(), "{}", output_text(&output));
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn test_names_digest(names: &[&str]) -> String {
    let mut encoder = Sha256::new();
    encoder.update((names.len() as u64).to_le_bytes());
    for name in names {
        encoder.update((name.len() as u64).to_le_bytes());
        encoder.update(name.as_bytes());
    }
    encoder
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn cache_entry(cache: &Path, fingerprint: &str) -> PathBuf {
    cache
        .join("llvm-ir/v2/sha256")
        .join(&fingerprint[..2])
        .join(&fingerprint[2..])
}

fn replace_metadata_field(metadata: String, field: &str, old: &str, new: &str) -> String {
    let before = format!("{field} = {old}");
    let after = format!("{field} = {new}");
    assert!(
        metadata.contains(&before),
        "missing metadata field `{before}`"
    );
    metadata.replacen(&before, &after, 1)
}

#[test]
fn help_and_version_identify_salic() {
    let help = salic().arg("--help").output().expect("run salic --help");
    assert!(help.status.success(), "{}", output_text(&help));
    assert!(String::from_utf8_lossy(&help.stdout).contains("salic build"));

    let version = salic()
        .arg("--version")
        .output()
        .expect("run salic --version");
    assert!(version.status.success(), "{}", output_text(&version));
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("salic "));
}

#[test]
fn formatter_is_idempotent_checks_without_writing_and_formats_packages() {
    let temporary = TestDirectory::new();
    let source = temporary.write(
        "main.sc",
        "let main(): i32 = {  \n// keep { here\nif true {\n42\n} else {\n0\n}\n}",
    );
    let original = fs::read_to_string(&source).expect("read unformatted source");
    let check = salic()
        .args(["fmt", "--check"])
        .arg(&source)
        .output()
        .expect("check formatting");
    assert_eq!(check.status.code(), Some(1), "{}", output_text(&check));
    assert_eq!(fs::read_to_string(&source).unwrap(), original);

    let formatted = salic()
        .arg("fmt")
        .arg(&source)
        .output()
        .expect("format source");
    assert_eq!(
        formatted.status.code(),
        Some(0),
        "{}",
        output_text(&formatted)
    );
    let expected =
        "let main(): i32 = {\n  // keep { here\n  if true {\n    42\n  } else {\n    0\n  }\n}\n";
    assert_eq!(fs::read_to_string(&source).unwrap(), expected);
    let checked = salic()
        .args(["fmt", "--check"])
        .arg(&source)
        .output()
        .expect("recheck formatting");
    assert_eq!(checked.status.code(), Some(0), "{}", output_text(&checked));

    let workspace = TestDirectory::new();
    workspace.write(
        "dep/salicin.toml",
        "[package]\nname = \"fmt-dep\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    let dependency = workspace.write("dep/src/lib.sc", "pub let answer(): i32 = {\n42\n}\n");
    workspace.write(
        "app/salicin.toml",
        "[package]\n\
         name = \"fmt-app\"\n\
         version = \"0.1.0\"\n\
         edition = \"2026\"\n\
         \n\
         [dependencies]\n\
         dep = { path = \"../dep\" }\n",
    );
    let main = workspace.write("app/src/main.sc", "let main(): i32 = {\ndep.answer()\n}\n");
    let module = workspace.write(
        "app/src/local.sc",
        "pub(package) let value(): i32 = {\n1\n}\n",
    );
    let package = salic()
        .arg("fmt")
        .arg(workspace.join("app"))
        .output()
        .expect("format package");
    assert_eq!(package.status.code(), Some(0), "{}", output_text(&package));
    assert!(fs::read_to_string(main)
        .unwrap()
        .contains("\n  dep.answer()\n"));
    assert!(fs::read_to_string(module).unwrap().contains("\n  1\n"));
    assert_eq!(
        fs::read_to_string(dependency).unwrap(),
        "pub let answer(): i32 = {\n42\n}\n",
        "formatting a package must not rewrite path dependencies"
    );
}

#[test]
fn formatter_rejects_invalid_source_without_writing() {
    let temporary = TestDirectory::new();
    let source = temporary.write("invalid.sc", "let main( = {\n");
    let output = salic()
        .arg("fmt")
        .arg(&source)
        .output()
        .expect("reject invalid source");
    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert_eq!(fs::read_to_string(source).unwrap(), "let main( = {\n");
}

#[test]
fn formatter_is_idempotent_across_the_passing_fixture_corpus() {
    for path in fixture_paths("pass") {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read '{}': {error}", path.display()));
        let formatted = format_source(&source)
            .unwrap_or_else(|error| panic!("format '{}': {error}", path.display()));
        let repeated = format_source(&formatted)
            .unwrap_or_else(|error| panic!("reformat '{}': {error}", path.display()));
        assert_eq!(repeated, formatted, "{} was not idempotent", path.display());
    }
}

#[test]
fn built_in_test_runner_links_one_binary_and_reports_the_failing_name() {
    let passing = salic()
        .arg("test")
        .arg(fixture("test", "multiple_pass.sc"))
        .output()
        .expect("run passing built-in tests");
    assert_eq!(passing.status.code(), Some(0), "{}", output_text(&passing));

    let failing = salic()
        .arg("test")
        .arg(fixture("test", "second_fails.sc"))
        .output()
        .expect("run failing built-in tests");
    assert_eq!(failing.status.code(), Some(1), "{}", output_text(&failing));
    assert!(
        String::from_utf8_lossy(&failing.stderr).contains("test \"second\" failed"),
        "{}",
        output_text(&failing)
    );

    let workspace = TestDirectory::new();
    workspace.write(
        "dep/salicin.toml",
        "[package]\n\
         name = \"test-dependency\"\n\
         version = \"0.1.0\"\n\
         edition = \"2026\"\n\
         \n\
         [lib]\n\
         path = \"src/lib.sc\"\n",
    );
    workspace.write(
        "dep/src/lib.sc",
        "test(\"dependency test must not run\") { std.test.fail(\"must not run\") }\n",
    );
    let app = workspace.create_dir("app");
    workspace.write(
        "app/salicin.toml",
        "[package]\n\
         name = \"test-app\"\n\
         version = \"0.1.0\"\n\
         edition = \"2026\"\n\
         \n\
         [dependencies]\n\
         dep = { path = \"../dep\" }\n",
    );
    workspace.write("app/src/main.sc", "test(\"primary package test\") { () }\n");
    let package = salic()
        .arg("test")
        .arg(app)
        .output()
        .expect("run primary package tests");
    assert_eq!(package.status.code(), Some(0), "{}", output_text(&package));

    let listed = salic()
        .args(["test", "--list"])
        .arg(workspace.join("app"))
        .output()
        .expect("list only primary package tests");
    assert_eq!(listed.status.code(), Some(0), "{}", output_text(&listed));
    assert_eq!(
        String::from_utf8_lossy(&listed.stdout),
        "primary package test\n"
    );
    assert!(listed.stderr.is_empty(), "{}", output_text(&listed));
}

#[test]
fn throwing_test_failures_report_all_messages_and_reject_unhandled_effects() {
    let temporary = TestDirectory::new();
    let source = temporary.write(
        "structured.sc",
        "test(\"messaged\") {\n\
           core.error.throw(\"盐: expected 42\")\n\
         }\n\
         test(\"empty message\") {\n\
           core.error.throw(\"\")\n\
         }\n\
         test(\"later registration\") {\n\
           core.error.throw(\"later still ran\")\n\
         }\n",
    );
    let output = salic()
        .arg("test")
        .arg(source)
        .output()
        .expect("run structured test failures");
    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "salic: test \"messaged\" failed: 盐: expected 42\n\
         salic: test \"empty message\" failed: \n\
         salic: test \"later registration\" failed: later still ran\n\
         salic: test result: 0 passed; 3 failed; 3 selected\n",
        "{}",
        output_text(&output)
    );

    let unrelated = temporary.write(
        "unrelated.sc",
        "let unrelated = effect { let escape(): () }\n\
         test(\"wrong effect\") {\n\
           unrelated.escape()\n\
           ()\n\
         }\n",
    );
    let rejected = salic()
        .arg("test")
        .arg(unrelated)
        .output()
        .expect("reject unrelated escaping test effect");
    assert_eq!(
        rejected.status.code(),
        Some(1),
        "{}",
        output_text(&rejected)
    );
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("requires custom effect `unrelated`"),
        "{}",
        output_text(&rejected)
    );
}

#[test]
fn standard_test_assertions_evaluate_once_and_report_stable_messages() {
    let temporary = TestDirectory::new();
    let passing = temporary.write(
        "assertions-pass.sc",
        "let evaluate(counter: borrow(mut)(i32)): i64 = {\n\
           counter = counter + 1\n\
           42\n\
         }\n\
         let common_assertions_pass: with(core.error.throwing(core.string.string))(): () = {\n\
           let mut counter = 0\n\
           std.test.assert(true)\n\
           std.test.assert_eq(evaluate(counter))(evaluate(counter))\n\
           let same: i64 = 42\n\
           let different: i64 = 43\n\
           std.test.assert_ne(i64)(same)(different)\n\
           let some_value: core.option(i32) = core.option.some(40)\n\
           let none_value: core.option(i64) = core.option.none\n\
           let ok_value: core.result(i64)(i32) = core.result.ok(2)\n\
           let error_value: core.result(i64)(i64) = core.result.err(7)\n\
           let some = std.test.expect_some(i32)(some_value)\n\
           std.test.expect_none(i64)(none_value)\n\
           let ok = std.test.expect_ok(i64, i32)(ok_value)\n\
           let error = std.test.expect_err(i64, i64)(error_value)\n\
           std.test.assert(some + ok == 42 && error == 7 && counter == 2)\n\
         }\n\
         test(\"common assertions pass\") { common_assertions_pass() }\n",
    );
    let passed = salic()
        .arg("test")
        .arg(passing)
        .output()
        .expect("run standard passing assertions");
    assert_eq!(passed.status.code(), Some(0), "{}", output_text(&passed));

    let failing = temporary.write(
        "assertions-fail.sc",
        "let fail_assert: with(core.error.throwing(core.string.string))(): () = {\n\
           std.test.assert(false)\n\
         }\n\
         let fail_assert_eq: with(core.error.throwing(core.string.string))(): () = {\n\
           let left: i64 = 1\n\
           let right: i64 = 2\n\
           std.test.assert_eq(i64)(left)(right)\n\
         }\n\
         let fail_assert_ne: with(core.error.throwing(core.string.string))(): () = {\n\
           let value: i64 = 7\n\
           std.test.assert_ne(i64)(value)(value)\n\
         }\n\
         let fail_expect_some: with(core.error.throwing(core.string.string))(): () = {\n\
           let value: core.option(i64) = core.option.none\n\
           let _ = std.test.expect_some(i64)(value)\n\
         }\n\
         let fail_expect_none: with(core.error.throwing(core.string.string))(): () = {\n\
           let value: core.option(i64) = core.option.some(9)\n\
           std.test.expect_none(i64)(value)\n\
         }\n\
         let fail_expect_ok: with(core.error.throwing(core.string.string))(): () = {\n\
           let value: core.result(i64)(i64) = core.result.err(0)\n\
           let _ = std.test.expect_ok(i64, i64)(value)\n\
         }\n\
         let fail_expect_err: with(core.error.throwing(core.string.string))(): () = {\n\
           let value: core.result(i64)(i64) = core.result.ok(11)\n\
           let _ = std.test.expect_err(i64, i64)(value)\n\
         }\n\
         test(\"assert\") { fail_assert() }\n\
         test(\"assert_eq\") { fail_assert_eq() }\n\
         test(\"assert_ne\") { fail_assert_ne() }\n\
         test(\"expect_some\") { fail_expect_some() }\n\
         test(\"expect_none\") { fail_expect_none() }\n\
         test(\"expect_ok\") { fail_expect_ok() }\n\
         test(\"expect_err\") { fail_expect_err() }\n\
         test(\"fail\") { std.test.fail(\"explicit failure\") }\n",
    );
    let failed = salic()
        .arg("test")
        .arg(failing)
        .output()
        .expect("run standard failing assertions");
    assert_eq!(failed.status.code(), Some(1), "{}", output_text(&failed));
    assert_eq!(
        String::from_utf8_lossy(&failed.stderr),
        "salic: test \"assert\" failed: assertion failed\n\
         salic: test \"assert_eq\" failed: assert_eq failed\n\
         left: 1\n\
         right: 2\n\
         salic: test \"assert_ne\" failed: assert_ne failed\n\
         both: 7\n\
         salic: test \"expect_some\" failed: expect_some failed: found none\n\
         salic: test \"expect_none\" failed: expect_none failed: found some(9)\n\
         salic: test \"expect_ok\" failed: expect_ok failed: found err(0)\n\
         salic: test \"expect_err\" failed: expect_err failed: found ok(11)\n\
         salic: test \"fail\" failed: explicit failure\n\
         salic: test result: 0 passed; 8 failed; 8 selected\n",
        "{}",
        output_text(&failed)
    );
}

#[test]
fn test_listing_filtering_counts_and_duplicate_names_are_deterministic() {
    let temporary = TestDirectory::new();
    let source = temporary.write(
        "selection.sc",
        "test(\"zeta\") { std.test.fail(\"zeta failed\") }\n\
         test(\"alpha\") { () }\n\
         test(\"alphabet\") { std.test.fail(\"alphabet failed\") }\n",
    );

    let listed = salic()
        .args(["test", "--list"])
        .arg(&source)
        .output()
        .expect("list tests");
    assert_eq!(listed.status.code(), Some(0), "{}", output_text(&listed));
    assert_eq!(
        String::from_utf8_lossy(&listed.stdout),
        "zeta\nalpha\nalphabet\n"
    );
    assert!(listed.stderr.is_empty(), "{}", output_text(&listed));

    let filtered_list = salic()
        .args(["test", "--list", "--filter", "alpha"])
        .arg(&source)
        .output()
        .expect("list filtered tests");
    assert_eq!(
        filtered_list.status.code(),
        Some(0),
        "{}",
        output_text(&filtered_list)
    );
    assert_eq!(
        String::from_utf8_lossy(&filtered_list.stdout),
        "alpha\nalphabet\n"
    );
    assert!(
        filtered_list.stderr.is_empty(),
        "{}",
        output_text(&filtered_list)
    );

    let multiple = salic()
        .args(["test", "--filter", "alpha"])
        .arg(&source)
        .output()
        .expect("run passing and failing substring matches");
    assert_eq!(
        multiple.status.code(),
        Some(1),
        "{}",
        output_text(&multiple)
    );
    assert_eq!(
        String::from_utf8_lossy(&multiple.stderr),
        "salic: test \"alphabet\" failed: alphabet failed\n\
         salic: test result: 1 passed; 1 failed; 2 selected\n"
    );

    let repeated = salic()
        .args(["test", "--filter", "alpha"])
        .arg(&source)
        .output()
        .expect("repeat deterministic filter");
    assert_eq!(repeated.stderr, multiple.stderr);

    let one = salic()
        .args(["test", "--filter", "zeta"])
        .arg(&source)
        .output()
        .expect("run one selected test");
    assert_eq!(one.status.code(), Some(1), "{}", output_text(&one));
    assert_eq!(
        String::from_utf8_lossy(&one.stderr),
        "salic: test \"zeta\" failed: zeta failed\n\
         salic: test result: 0 passed; 1 failed; 1 selected\n"
    );

    let missing = salic()
        .args(["test", "--filter", "missing"])
        .arg(&source)
        .output()
        .expect("run an empty selection");
    assert_eq!(missing.status.code(), Some(0), "{}", output_text(&missing));
    assert_eq!(
        String::from_utf8_lossy(&missing.stderr),
        "salic: test result: 0 passed; 0 failed; 0 selected\n"
    );

    let same_source = temporary.write(
        "duplicate.sc",
        "test(\"same name\") { () }\n\
         test(\"same name\") { () }\n",
    );
    let same_source_rejected = salic()
        .args(["test", "--list"])
        .arg(same_source)
        .output()
        .expect("reject same-source duplicate test names");
    assert_eq!(
        same_source_rejected.status.code(),
        Some(1),
        "{}",
        output_text(&same_source_rejected)
    );
    let same_source_diagnostic = String::from_utf8_lossy(&same_source_rejected.stderr);
    assert!(
        same_source_diagnostic.contains("duplicate test registration name \"same name\""),
        "{}",
        output_text(&same_source_rejected)
    );
    assert!(
        !same_source_diagnostic.contains("$test$"),
        "{}",
        output_text(&same_source_rejected)
    );

    let duplicate = TestDirectory::new();
    duplicate.write(
        "salicin.toml",
        "[package]\nname = \"duplicate-tests\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    duplicate.write("src/main.sc", "test(\"same name\") { () }\n");
    duplicate.write("src/other.sc", "test(\"same name\") { () }\n");
    let rejected = salic()
        .args(["test", "--list"])
        .arg(&duplicate.0)
        .output()
        .expect("reject duplicate test names");
    assert_eq!(
        rejected.status.code(),
        Some(1),
        "{}",
        output_text(&rejected)
    );
    let diagnostic = String::from_utf8_lossy(&rejected.stderr);
    assert!(
        diagnostic.contains("duplicate test registration name \"same name\""),
        "{}",
        output_text(&rejected)
    );
    assert!(!diagnostic.contains("$test$"), "{}", output_text(&rejected));
}

#[test]
fn structured_test_abort_runs_owned_cleanup_once() {
    let temporary = TestDirectory::new();
    let source = temporary.write(
        "cleanup.sc",
        "pub let resource = struct { pub counter: ptr(mut)(i32) }\n\
         extend(resource, droppable) {\n\
           let drop(self: borrow(mut)(self))(): () = {\n\
             unsafe { *self.counter = *self.counter + 1 }\n\
           }\n\
         }\n\
         let abort(counter: ptr(mut)(i32)): core.testing.outcome = {\n\
           core.testing.run {\n\
             let owned = resource { counter: counter }\n\
             core.error.throw(\"cleanup probe\")\n\
           }\n\
         }\n\
         let main(): i32 = {\n\
           let counter = unsafe { raw_alloc(i32)(size_of(i32), align_of(i32)) }\n\
           unsafe { *counter = 0 }\n\
           let result = abort(counter)\n\
           let drops = unsafe { *counter }\n\
           unsafe { raw_dealloc(counter, size_of(i32), align_of(i32)) }\n\
           match result\n\
             { passed -> 1 }\n\
             { failed(_) -> if drops == 1 { 42 } else { 2 } }\n\
         }\n",
    );
    let output = salic()
        .arg("run")
        .arg(source)
        .output()
        .expect("run structured test cleanup probe");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn source_extension_is_sc_without_a_legacy_alias() {
    let temporary = TestDirectory::new();
    let legacy = temporary.write("legacy.sali", "let main(): i32 = { 42 }\n");
    let output = salic()
        .arg("check")
        .arg(legacy)
        .output()
        .expect("check rejected legacy source extension");
    assert_eq!(output.status.code(), Some(2), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must be a .sc file"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn core_diagnostics_are_stable_source_level_contracts() {
    for (fixture_name, line, column, message) in [
        (
            "use_after_move.sc",
            8,
            3,
            "use of moved or uninitialized value",
        ),
        (
            "array_index_type.sc",
            3,
            3,
            "type mismatch for array index: expected `usize`, found `bool`",
        ),
        (
            "throw_in_plain_return.sc",
            2,
            3,
            "call to `throw` requires `throwing(bool)`; handle it with `try { ... }` or propagate it from the current function",
        ),
    ] {
        let source = fixture("fail", fixture_name);
        let output = salic()
            .arg("check")
            .arg(&source)
            .output()
            .expect("check diagnostic contract fixture");
        assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!(
                "{}:{line}:{column}: error: {message}\n",
                source.display()
            ),
            "{}",
            output_text(&output)
        );
    }
}

#[test]
fn compiler_ir_symbols_and_diagnostics_are_byte_deterministic() {
    let ledger =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/ledger.sc"))
            .expect("read ledger acceptance source");
    let baseline_ir = compile_source(&ledger).expect("compile ledger acceptance source");
    let baseline_symbols = baseline_ir
        .lines()
        .filter(|line| line.starts_with("define ") || line.starts_with("declare "))
        .collect::<Vec<_>>();

    for _ in 0..3 {
        let ir = compile_source(&ledger).expect("recompile ledger acceptance source");
        assert_eq!(
            ir, baseline_ir,
            "LLVM IR changed between identical compiles"
        );
        let symbols = ir
            .lines()
            .filter(|line| line.starts_with("define ") || line.starts_with("declare "))
            .collect::<Vec<_>>();
        assert_eq!(
            symbols, baseline_symbols,
            "LLVM symbol order changed between identical compiles"
        );
    }

    for name in [
        "generic_nominal_recursive_layout.sc",
        "trait_ambiguous_method.sc",
        "standard_throw_ambiguous_failure.sc",
    ] {
        let source = fs::read_to_string(fixture("fail", name)).expect("read failure fixture");
        let baseline = check_source(&source).expect_err("fixture must fail source checking");
        for _ in 0..3 {
            assert_eq!(
                check_source(&source).expect_err("fixture must fail source checking"),
                baseline,
                "{name} diagnostics changed between identical checks"
            );
        }
    }
}

#[test]
fn unicode_identifiers_and_logical_newlines_run_natively() {
    for (name, output) in
        batched_native_fixture_outputs(&["unicode_identifiers.sc", "logical_newlines.sc"])
    {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn parenthesis_free_unary_calls_preserve_groups_and_precedence() {
    for (name, output) in batched_native_fixture_outputs(&["parenthesis_free_unary_calls.sc"]) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "parenthesis_free_multi_parameter_group.sc"))
        .output()
        .expect("reject a bare call split across a multi-parameter group");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("too many parameter groups in call to `add`"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn malformed_unicode_source_and_confusable_module_names_are_diagnostic() {
    let temporary = TestDirectory::new();
    let malformed = temporary.write("malformed.sc", "let ab\u{200b}cd = 1\n");
    let output = salic()
        .arg("check")
        .arg(&malformed)
        .output()
        .expect("check malformed Unicode source");
    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}: error: 1:7: unexpected character `\u{200b}`\n",
            malformed.display()
        )
    );

    temporary.write(
        "project/salicin.toml",
        "[package]\nname = \"unicode-boundary\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    temporary.write("project/src/main.sc", "let main(): i32 = { 42 }\n");
    temporary.write("project/src/mоdule.sc", "let value = 1\n");
    let project = temporary.join("project");
    let output = salic()
        .arg("check")
        .arg(&project)
        .output()
        .expect("check confusable module path");
    assert_eq!(output.status.code(), Some(2), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(
            "file module segment `mоdule` in 'mоdule.sc' must be a non-reserved ASCII snake_case identifier"
        ),
        "{}",
        output_text(&output)
    );
}

#[test]
fn return_type_effect_groups_run_with_expected_result() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "return_type_effects.sc"))
        .output()
        .expect("run return-type effect fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn ledger_example_exercises_the_m0_core_as_a_complete_program() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/ledger.sc");
    let output = salic()
        .arg("run")
        .arg(source)
        .output()
        .expect("run M0 ledger example");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn inventory_example_is_a_practical_multimodule_standard_library_acceptance() {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/inventory");
    let temporary = TestDirectory::new();
    let executable = temporary.join("inventory");

    let checked = salic()
        .arg("check")
        .arg(&package)
        .output()
        .expect("check inventory acceptance example");
    assert!(checked.status.success(), "{}", output_text(&checked));

    let tests = salic()
        .arg("test")
        .arg(&package)
        .output()
        .expect("run inventory source assertions");
    assert_eq!(tests.status.code(), Some(0), "{}", output_text(&tests));
    assert_eq!(
        String::from_utf8_lossy(&tests.stderr),
        "salic: test result: 4 passed; 0 failed; 4 selected\n"
    );

    let built = salic()
        .arg("build")
        .arg(&package)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("build inventory acceptance example");
    assert!(built.status.success(), "{}", output_text(&built));

    let expected = b"items=2\ntotal=41\nname_bytes=4\n";
    for name in ["first.txt", "second.txt"] {
        let report = temporary.join(name);
        let output = Command::new(&executable)
            .arg(&report)
            .args(["A", "2", "10", "柳", "3", "7"])
            .output()
            .expect("run inventory acceptance example");
        assert_eq!(output.status.code(), Some(0), "{}", output_text(&output));
        assert_eq!(output.stdout, expected);
        assert!(output.stderr.is_empty(), "{}", output_text(&output));
        assert_eq!(fs::read(report).expect("read inventory report"), expected);
    }

    let invalid = Command::new(&executable)
        .args(["unused.txt", "A", "not-a-number", "10", "柳", "3", "7"])
        .output()
        .expect("run invalid inventory input");
    assert_eq!(invalid.status.code(), Some(3), "{}", output_text(&invalid));
    assert!(invalid.stdout.is_empty(), "{}", output_text(&invalid));
    assert_eq!(invalid.stderr, b"invalid first units\n");

    let missing = Command::new(&executable)
        .output()
        .expect("run inventory without arguments");
    assert_eq!(missing.status.code(), Some(2), "{}", output_text(&missing));
    assert_eq!(
        missing.stderr,
        b"usage: inventory OUTPUT NAME UNITS PRICE NAME UNITS PRICE\n"
    );
}
