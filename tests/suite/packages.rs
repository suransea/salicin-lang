use crate::support::*;

#[test]
fn output_must_not_overwrite_the_source() {
    let temporary = TestDirectory::new();
    let source = temporary.join("keep.sc");
    let original = b"let main(): i32 = { 0 }\n";
    fs::write(&source, original).expect("write source fixture");

    for command in ["build", "emit-ir"] {
        let output = salic()
            .arg(command)
            .arg(&source)
            .arg("-o")
            .arg(&source)
            .output()
            .expect("run salic");
        assert_eq!(output.status.code(), Some(2), "{}", output_text(&output));
        assert_eq!(fs::read(&source).expect("read preserved source"), original);
    }
}

#[test]
fn output_must_not_overwrite_a_source_hardlink() {
    let temporary = TestDirectory::new();
    let source = temporary.join("keep.sc");
    let output_path = temporary.join("keep.ll");
    let original = b"let main(): i32 = { 0 }\n";
    fs::write(&source, original).expect("write source fixture");
    fs::hard_link(&source, &output_path).expect("create source hardlink");

    let output = salic()
        .arg("emit-ir")
        .arg(&source)
        .arg("-o")
        .arg(&output_path)
        .output()
        .expect("reject output hardlink");
    assert_eq!(output.status.code(), Some(2), "{}", output_text(&output));
    assert_eq!(fs::read(&source).expect("read preserved source"), original);
    assert_eq!(
        fs::read(&output_path).expect("read preserved hardlink"),
        original
    );
}

#[test]
fn package_default_target_accepts_directory_manifest_and_cwd_discovery() {
    let project = TestDirectory::new();
    let manifest = project.write(
        "salicin.toml",
        r#"[package]
name = "hello-salicin"
version = "0.1.0"
edition = "2026"
"#,
    );
    project.write("src/main.sc", "let main(): i32 = { 42 }\n");

    let checked = salic()
        .arg("check")
        .arg(&project.0)
        .output()
        .expect("check package directory");
    assert!(checked.status.success(), "{}", output_text(&checked));

    let run = salic()
        .arg("run")
        .arg(&manifest)
        .output()
        .expect("run package manifest");
    assert_eq!(run.status.code(), Some(42), "{}", output_text(&run));

    let built = salic()
        .arg("build")
        .arg(&project.0)
        .output()
        .expect("build package directory");
    assert!(built.status.success(), "{}", output_text(&built));
    let mut executable = project.join("build/hello-salicin");
    if !std::env::consts::EXE_EXTENSION.is_empty() {
        executable.set_extension(std::env::consts::EXE_EXTENSION);
    }
    assert!(
        executable.is_file(),
        "default package output was not written to {}",
        executable.display()
    );

    let nested = project.create_dir("src/nested/deeper");
    let discovered = salic()
        .arg("run")
        .current_dir(nested)
        .output()
        .expect("run package discovered from cwd");
    assert_eq!(
        discovered.status.code(),
        Some(42),
        "{}",
        output_text(&discovered)
    );
}

#[test]
fn explicit_package_targets_support_bin_selection_and_lib_checking() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        r#"[package]
name = "toolbox"
version = "0.1.0"
edition = "2026"

[lib]
path = "src/toolbox.sc"

[[bin]]
name = "toolbox"
path = "src/main.sc"

[[bin]]
name = "answer"
path = "src/answer.sc"
"#,
    );
    project.write("src/toolbox.sc", "let answer(): i32 = { 42 }\n");
    project.write("src/main.sc", "let main(): i32 = { 1 }\n");
    project.write("src/answer.sc", "let main(): i32 = { 42 }\n");

    let checked = salic()
        .arg("check")
        .arg("--lib")
        .arg(&project.0)
        .output()
        .expect("check explicitly selected library target");
    assert!(checked.status.success(), "{}", output_text(&checked));

    let run = salic()
        .arg("run")
        .arg(&project.0)
        .args(["--bin", "answer"])
        .output()
        .expect("run explicitly selected binary target");
    assert_eq!(run.status.code(), Some(42), "{}", output_text(&run));

    let built = salic()
        .arg("build")
        .arg(&project.0)
        .args(["--bin", "answer"])
        .output()
        .expect("build explicitly selected binary target");
    assert!(built.status.success(), "{}", output_text(&built));
    let mut executable = project.join("build/answer");
    if !std::env::consts::EXE_EXTENSION.is_empty() {
        executable.set_extension(std::env::consts::EXE_EXTENSION);
    }
    assert!(executable.is_file(), "missing {}", executable.display());
}

#[test]
fn package_target_selection_errors_explain_how_to_resolve_them() {
    let multiple_bins = TestDirectory::new();
    multiple_bins.write(
        "salicin.toml",
        r#"[package]
name = "ambiguous"
version = "0.1.0"
edition = "2026"

[[bin]]
name = "left"
path = "src/left.sc"

[[bin]]
name = "right"
path = "src/right.sc"
"#,
    );
    multiple_bins.write("src/left.sc", "let main(): i32 = { 1 }\n");
    multiple_bins.write("src/right.sc", "let main(): i32 = { 2 }\n");

    let ambiguous = salic()
        .arg("run")
        .arg(&multiple_bins.0)
        .output()
        .expect("run package with ambiguous binary target");
    assert!(
        !ambiguous.status.success(),
        "ambiguous target unexpectedly ran"
    );
    let stderr = String::from_utf8_lossy(&ambiguous.stderr).to_lowercase();
    assert!(
        stderr.contains("--bin") && (stderr.contains("multiple") || stderr.contains("choose")),
        "{}",
        output_text(&ambiguous)
    );
    assert!(!multiple_bins.join("salicin.lock").exists());

    let library_only = TestDirectory::new();
    library_only.write(
        "salicin.toml",
        r#"[package]
name = "library-only"
version = "0.1.0"
edition = "2026"

[lib]
path = "src/lib.sc"
"#,
    );
    library_only.write("src/lib.sc", "let answer(): i32 = { 42 }\n");

    let no_binary = salic()
        .arg("run")
        .arg(&library_only.0)
        .output()
        .expect("run library-only package");
    assert!(
        !no_binary.status.success(),
        "library-only package unexpectedly ran"
    );
    let stderr = String::from_utf8_lossy(&no_binary.stderr).to_lowercase();
    assert!(
        stderr.contains("bin") || stderr.contains("binary"),
        "{}",
        output_text(&no_binary)
    );
}

#[test]
fn invalid_manifests_and_dependencies_fail_with_context() {
    let invalid_manifest = TestDirectory::new();
    invalid_manifest.write(
        "salicin.toml",
        "[package\nname = \"broken\"\nversion = \"0.1.0\"\n",
    );

    let malformed = salic()
        .arg("check")
        .arg(&invalid_manifest.0)
        .output()
        .expect("check package with malformed manifest");
    assert!(
        !malformed.status.success(),
        "malformed manifest unexpectedly passed"
    );
    let stderr = String::from_utf8_lossy(&malformed.stderr).to_lowercase();
    assert!(
        stderr.contains("manifest") || stderr.contains("salicin.toml"),
        "{}",
        output_text(&malformed)
    );

    let invalid_dependency = TestDirectory::new();
    invalid_dependency.write(
        "salicin.toml",
        r#"[package]
name = "bad-dependency"
version = "0.1.0"
edition = "2026"

[dependencies]
broken = 42
"#,
    );
    invalid_dependency.write("src/main.sc", "let main(): i32 = { 0 }\n");

    let dependency = salic()
        .arg("check")
        .arg(&invalid_dependency.0)
        .output()
        .expect("check package with invalid dependency declaration");
    assert!(
        !dependency.status.success(),
        "invalid dependency unexpectedly passed"
    );
    let stderr = String::from_utf8_lossy(&dependency.stderr).to_lowercase();
    assert!(stderr.contains("depend"), "{}", output_text(&dependency));
}

#[test]
fn local_path_dependency_runs_only_its_library_and_writes_a_stable_lockfile() {
    let workspace = TestDirectory::new();
    workspace.write(
        "app/salicin.toml",
        r#"[package]
name = "dependency-app"
version = "0.1.0"
edition = "2026"

[dependencies]
math = { path = "../math" }
"#,
    );
    workspace.write("app/src/main.sc", "let main(): i32 = { math.answer() }\n");
    workspace.write(
        "app/src/lib.sc",
        "pub let library_answer(): i32 = { math.answer() }\n",
    );
    workspace.write(
        "math/salicin.toml",
        r#"[package]
name = "math-library"
version = "1.2.3"
edition = "2026"

[[bin]]
name = "broken-tool"
path = "src/broken.sc"
"#,
    );
    let dependency_library = "pub let answer(): i32 = { internal.value() }\n";
    let dependency_library_path = workspace.write("math/src/lib.sc", dependency_library);
    workspace.write(
        "math/src/internal.sc",
        "pub(package) let value(): i32 = { 42 }\n",
    );
    workspace.write(
        "math/src/broken.sc",
        "this deliberately is not valid Salicin\n",
    );

    let app = workspace.join("app");
    let run = salic()
        .arg("run")
        .arg(&app)
        .output()
        .expect("run package with a local dependency");
    assert_eq!(run.status.code(), Some(42), "{}", output_text(&run));

    for command in ["check", "emit-ir"] {
        let library = salic()
            .arg(command)
            .arg("--lib")
            .arg(&app)
            .output()
            .expect("compile the root library with its dependency");
        assert!(library.status.success(), "{}", output_text(&library));
    }

    let lock_path = app.join("salicin.lock");
    let first = fs::read_to_string(&lock_path).expect("read generated lockfile");
    assert_eq!(first.matches("[[package]]").count(), 2, "{first}");
    assert!(first.contains("name = \"math-library\""), "{first}");
    assert!(first.contains("path = \"../math\""), "{first}");

    fs::remove_file(&lock_path).expect("remove generated lockfile before regeneration");
    let checked = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("check package again");
    assert!(checked.status.success(), "{}", output_text(&checked));
    assert_eq!(fs::read_to_string(&lock_path).unwrap(), first);

    let overwrite = salic()
        .arg("emit-ir")
        .arg(&app)
        .arg("-o")
        .arg(&lock_path)
        .output()
        .expect("reject lockfile overwrite");
    assert_eq!(
        overwrite.status.code(),
        Some(2),
        "{}",
        output_text(&overwrite)
    );
    assert_eq!(fs::read_to_string(&lock_path).unwrap(), first);

    let dependency_overwrite = salic()
        .arg("emit-ir")
        .arg(&app)
        .arg("-o")
        .arg(&dependency_library_path)
        .output()
        .expect("reject dependency source overwrite");
    assert_eq!(
        dependency_overwrite.status.code(),
        Some(2),
        "{}",
        output_text(&dependency_overwrite)
    );
    assert_eq!(
        fs::read_to_string(&dependency_library_path).unwrap(),
        dependency_library
    );
}

#[test]
fn locked_and_frozen_require_a_current_valid_lockfile() {
    let project = TestDirectory::new();
    let manifest = project.write(
        "salicin.toml",
        "[package]\nname = \"locked-app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    project.write("src/main.sc", "let main(): i32 = { 0 }\n");
    let lock = project.join("salicin.lock");

    for mode in ["--locked", "--frozen"] {
        let missing = salic()
            .arg("check")
            .arg(&project.0)
            .arg(mode)
            .output()
            .expect("reject missing required lockfile");
        assert_eq!(missing.status.code(), Some(2), "{}", output_text(&missing));
        assert!(
            String::from_utf8_lossy(&missing.stderr).contains("is required"),
            "{}",
            output_text(&missing)
        );
        assert!(!lock.exists(), "{mode} must not create a lockfile");
    }

    let generated = salic()
        .arg("check")
        .arg(&project.0)
        .output()
        .expect("generate lockfile");
    assert!(generated.status.success(), "{}", output_text(&generated));
    let original = fs::read_to_string(&lock).expect("read generated lockfile");

    for mode in ["--locked", "--frozen"] {
        let current = salic()
            .arg("check")
            .arg(&project.0)
            .arg(mode)
            .output()
            .expect("accept current lockfile");
        assert!(current.status.success(), "{}", output_text(&current));
        assert_eq!(fs::read_to_string(&lock).unwrap(), original);
    }

    fs::write(
        &manifest,
        "[package]\nname = \"locked-app\"\nversion = \"0.2.0\"\nedition = \"2026\"\n",
    )
    .expect("change manifest resolution input");
    let stale = salic()
        .arg("check")
        .arg(&project.0)
        .arg("--locked")
        .output()
        .expect("reject stale lockfile");
    assert_eq!(stale.status.code(), Some(2), "{}", output_text(&stale));
    assert!(
        String::from_utf8_lossy(&stale.stderr).contains("out of date"),
        "{}",
        output_text(&stale)
    );
    assert_eq!(fs::read_to_string(&lock).unwrap(), original);

    fs::write(&lock, "format = 1\n").expect("damage lockfile");
    let malformed = salic()
        .arg("check")
        .arg(&project.0)
        .arg("--frozen")
        .output()
        .expect("reject malformed lockfile");
    assert_eq!(
        malformed.status.code(),
        Some(2),
        "{}",
        output_text(&malformed)
    );
    assert!(
        String::from_utf8_lossy(&malformed.stderr).contains("unsupported lockfile format"),
        "{}",
        output_text(&malformed)
    );

    let conflicting = salic()
        .arg("check")
        .arg(&project.0)
        .arg("--locked")
        .arg("--frozen")
        .output()
        .expect("reject conflicting lock modes");
    assert_eq!(
        conflicting.status.code(),
        Some(2),
        "{}",
        output_text(&conflicting)
    );
}

#[test]
fn incremental_fingerprint_is_path_independent_and_input_sensitive() {
    fn write_project(project: &TestDirectory) {
        project.write(
            "salicin.toml",
            "[package]\nname = \"fingerprint-app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
        );
        project.write("src/main.sc", "let main(): i32 = { shared.answer() }\n");
        project.write(
            "src/lib.sc",
            "pub let library_answer(): i32 = { shared.answer() }\n",
        );
        project.write("src/shared.sc", "pub(package) let answer(): i32 = { 42 }\n");
    }

    fn fingerprint(project: &TestDirectory, extra: &[&str]) -> Output {
        let mut command = salic();
        command.arg("fingerprint").arg(&project.0);
        command.args(extra);
        command.output().expect("fingerprint project")
    }

    let first = TestDirectory::new();
    let relocated = TestDirectory::new();
    write_project(&first);
    write_project(&relocated);

    let first_binary = fingerprint(&first, &[]);
    let relocated_binary = fingerprint(&relocated, &[]);
    assert!(
        first_binary.status.success(),
        "{}",
        output_text(&first_binary)
    );
    assert!(
        relocated_binary.status.success(),
        "{}",
        output_text(&relocated_binary)
    );
    assert_eq!(first_binary.stdout, relocated_binary.stdout);
    let hex = String::from_utf8_lossy(&first_binary.stdout);
    assert_eq!(hex.trim().len(), 64, "{hex}");
    assert!(hex.trim().bytes().all(|byte| byte.is_ascii_hexdigit()));

    let library = fingerprint(&first, &["--lib", "--locked"]);
    assert!(library.status.success(), "{}", output_text(&library));
    assert_ne!(first_binary.stdout, library.stdout);

    relocated.write("src/shared.sc", "pub(package) let answer(): i32 = { 43 }\n");
    let changed = fingerprint(&relocated, &["--locked"]);
    assert!(changed.status.success(), "{}", output_text(&changed));
    assert_ne!(first_binary.stdout, changed.stdout);
}

#[test]
fn incremental_cli_invalidates_each_package_graph_identity_dimension() {
    fn write_project(project: &TestDirectory) {
        project.write(
            "salicin.toml",
            "[package]\nname = \"app\"\nversion = \"1.0.0\"\nedition = \"2026\"\n\
             \n[dependencies]\nmath = { path = \"dep\" }\n",
        );
        project.write("src/main.sc", "let main(): i32 = { 0 }\n");
        project.write("src/feature.sc", "pub(package) let value(): i32 = { 1 }\n");
        project.write(
            "dep/salicin.toml",
            "[package]\nname = \"math\"\nversion = \"1.0.0\"\nedition = \"2026\"\n",
        );
        project.write("dep/src/lib.sc", "pub let answer(): i32 = { 42 }\n");
    }
    fn fingerprint(project: &TestDirectory) -> Vec<u8> {
        let output = salic().arg("fingerprint").arg(&project.0).output().unwrap();
        assert!(output.status.success(), "{}", output_text(&output));
        output.stdout
    }

    let baseline = TestDirectory::new();
    let alias = TestDirectory::new();
    let provider = TestDirectory::new();
    let module = TestDirectory::new();
    let source = TestDirectory::new();
    for project in [&baseline, &alias, &provider, &module, &source] {
        write_project(project);
    }
    alias.write(
        "salicin.toml",
        "[package]\nname = \"app\"\nversion = \"1.0.0\"\nedition = \"2026\"\n\
         \n[dependencies]\narithmetic = { path = \"dep\" }\n",
    );
    provider.write(
        "dep/salicin.toml",
        "[package]\nname = \"math\"\nversion = \"1.0.1\"\nedition = \"2026\"\n",
    );
    fs::rename(module.join("src/feature.sc"), module.join("src/renamed.sc")).unwrap();
    source.write(
        "src/feature.sc",
        "pub(package) let value(): i32 = { 1 }\n// byte change\n",
    );

    let baseline = fingerprint(&baseline);
    for changed in [
        fingerprint(&alias),
        fingerprint(&provider),
        fingerprint(&module),
        fingerprint(&source),
    ] {
        assert_ne!(baseline, changed);
    }
}

#[test]
fn virtual_workspace_selects_packages_and_shares_lock_and_build_roots() {
    let workspace = TestDirectory::new();
    workspace.write(
        "salicin.toml",
        "[workspace]\nmembers = [\"packages/app\", \"packages/math\", \"packages/tooling\"]\n",
    );
    workspace.write(
        "packages/app/salicin.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\nmath = { path = \"../math\" }\n",
    );
    workspace.write(
        "packages/app/src/main.sc",
        "let main(): i32 = { math.answer() }\n",
    );
    workspace.write(
        "packages/math/salicin.toml",
        "[package]\nname = \"math\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    workspace.write(
        "packages/math/src/lib.sc",
        "pub let answer(): i32 = { 42 }\n",
    );
    workspace.write(
        "packages/tooling/salicin.toml",
        "[package]\nname = \"tooling\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    workspace.write(
        "packages/tooling/src/lib.sc",
        "pub let independent(): i32 = { 7 }\n",
    );

    let ambiguous = salic()
        .arg("check")
        .arg(&workspace.0)
        .output()
        .expect("reject ambiguous virtual workspace");
    assert_eq!(
        ambiguous.status.code(),
        Some(2),
        "{}",
        output_text(&ambiguous)
    );
    assert!(
        String::from_utf8_lossy(&ambiguous.stderr).contains("--package"),
        "{}",
        output_text(&ambiguous)
    );

    let run = salic()
        .arg("run")
        .arg(&workspace.0)
        .arg("-p")
        .arg("app")
        .output()
        .expect("run selected workspace package");
    assert_eq!(run.status.code(), Some(42), "{}", output_text(&run));
    let lock = fs::read_to_string(workspace.join("salicin.lock")).expect("read workspace lock");
    assert_eq!(lock.matches("[[package]]").count(), 3, "{lock}");
    assert!(lock.contains("name = \"tooling\""), "{lock}");
    assert!(
        lock.contains("source = \"workspace:packages/app\""),
        "{lock}"
    );
    assert!(
        lock.contains("source = \"workspace:packages/tooling\""),
        "{lock}"
    );
    assert!(!workspace.join("packages/app/salicin.lock").exists());

    fs::remove_file(workspace.join("salicin.lock")).expect("remove workspace lock");
    let direct_member = salic()
        .arg("check")
        .arg(workspace.join("packages/app"))
        .output()
        .expect("select a member while retaining workspace ownership");
    assert!(
        direct_member.status.success(),
        "{}",
        output_text(&direct_member)
    );
    assert!(workspace.join("salicin.lock").is_file());
    assert!(!workspace.join("packages/app/salicin.lock").exists());

    let build = salic()
        .arg("build")
        .arg(&workspace.0)
        .arg("--package")
        .arg("app")
        .output()
        .expect("build selected workspace package");
    assert!(build.status.success(), "{}", output_text(&build));
    let executable = if cfg!(windows) { "app.exe" } else { "app" };
    assert!(workspace.join(&format!("build/{executable}")).is_file());
    assert!(!workspace.join("packages/app/build").exists());

    let missing = salic()
        .arg("check")
        .arg(&workspace.0)
        .arg("-p")
        .arg("missing")
        .output()
        .expect("reject unknown workspace package");
    assert_eq!(missing.status.code(), Some(2), "{}", output_text(&missing));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("available packages: app, math, tooling"),
        "{}",
        output_text(&missing)
    );
}

#[test]
fn rooted_workspace_defaults_to_its_root_package_and_formats_members() {
    let workspace = TestDirectory::new();
    workspace.write(
        "salicin.toml",
        "[package]\nname = \"root-app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[workspace]\nmembers = [\"member\"]\n",
    );
    workspace.write("src/main.sc", "let main():i32={13}\n");
    workspace.write(
        "member/salicin.toml",
        "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    let member_source = workspace.write("member/src/lib.sc", "pub let answer():i32={\n7\n}\n");

    let run = salic()
        .arg("run")
        .arg(&workspace.0)
        .output()
        .expect("run root workspace package by default");
    assert_eq!(run.status.code(), Some(13), "{}", output_text(&run));

    let format = salic()
        .arg("fmt")
        .arg(&workspace.0)
        .output()
        .expect("format all workspace members");
    assert!(format.status.success(), "{}", output_text(&format));
    let formatted = fs::read_to_string(member_source).expect("read formatted member");
    assert!(formatted.contains("\n  7\n"), "{formatted}");
}

#[test]
fn dependency_visibility_stops_package_and_private_items_at_the_boundary() {
    let workspace = TestDirectory::new();
    workspace.write(
        "app/salicin.toml",
        r#"[package]
name = "visibility-app"
version = "0.1.0"
edition = "2026"

[dependencies]
restricted = { path = "../restricted" }
secret = { path = "../secret" }
"#,
    );
    workspace.write(
        "app/src/main.sc",
        "let main(): i32 = { restricted.hidden() + secret.hidden() }\n",
    );
    workspace.write(
        "restricted/salicin.toml",
        "[package]\nname = \"restricted\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    workspace.write(
        "restricted/src/lib.sc",
        "pub(package) let hidden(): i32 = { 20 }\n",
    );
    workspace.write(
        "secret/salicin.toml",
        "[package]\nname = \"secret\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    workspace.write("secret/src/lib.sc", "let hidden(): i32 = { 22 }\n");

    let output = salic()
        .arg("check")
        .arg(workspace.join("app"))
        .output()
        .expect("check cross-package visibility");
    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert!(workspace.join("app/salicin.lock").is_file());
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(stderr.contains("pub(package)"), "{}", output_text(&output));
    assert!(stderr.contains("private"), "{}", output_text(&output));
}

#[test]
fn private_dependency_traits_do_not_leak_method_candidates() {
    let workspace = TestDirectory::new();
    workspace.write(
        "app/salicin.toml",
        r#"[package]
name = "trait-privacy-app"
version = "0.1.0"
edition = "2026"

[dependencies]
dep = { path = "../dep" }
"#,
    );
    workspace.write(
        "dep/salicin.toml",
        "[package]\nname = \"trait-privacy-dep\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    workspace.write(
        "dep/src/lib.sc",
        r#"use core.option

pub let number = struct { value: i32 }
let secret = trait {
  let reveal(self: borrow(self))(): i32
}
extend(number, secret) {
  let reveal(self: borrow(self))(): i32 = { self.value }
}
pub let make(): number = { number { value: 21 } }
pub let maybe(): option(number) = { option(number).some(make()) }
pub let reveal(comptime t: type)(move number: number): i32 = { number.reveal() }
pub let answer(): i32 = {
  let number = make()
  number.reveal()
}
"#,
    );
    workspace.write(
        "app/src/main.sc",
        r#"let main(): i32 = {
  let number = dep.make()
  number.reveal()
}
"#,
    );

    let app = workspace.join("app");
    let denied = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a private dependency trait method");
    assert_eq!(denied.status.code(), Some(1), "{}", output_text(&denied));
    let stderr = String::from_utf8_lossy(&denied.stderr).to_lowercase();
    assert!(
        stderr.contains("trait method") && stderr.contains("private"),
        "{}",
        output_text(&denied)
    );

    workspace.write(
        "app/src/main.sc",
        "let main(): i32 = { dep.maybe()?.reveal() ?? 0 }\n",
    );
    let optional = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject an optionally chained private trait method");
    assert_eq!(
        optional.status.code(),
        Some(1),
        "{}",
        output_text(&optional)
    );
    assert!(
        String::from_utf8_lossy(&optional.stderr)
            .to_lowercase()
            .contains("private"),
        "{}",
        output_text(&optional)
    );

    workspace.write(
        "app/src/main.sc",
        "let main(): i32 = { dep.reveal(i32)(dep.make()) + dep.answer() }\n",
    );
    let internal = salic()
        .arg("run")
        .arg(&app)
        .output()
        .expect("run dependency code using its own private trait");
    assert_eq!(
        internal.status.code(),
        Some(42),
        "{}",
        output_text(&internal)
    );
}

#[test]
fn embedded_core_lang_items_are_shared_but_module_names_cannot_spoof_them() {
    let workspace = TestDirectory::new();
    workspace.write(
        "app/salicin.toml",
        r#"[package]
name = "core-app"
version = "0.1.0"
edition = "2026"

[dependencies]
dep = { path = "../dep" }
"#,
    );
    workspace.write(
        "dep/salicin.toml",
        "[package]\nname = \"core-dependency\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    workspace.write(
        "dep/src/lib.sc",
        r#"use core.option
let add = core.ops.add

pub let number = struct { value: i32 }
extend(number, add(number)) {
  let output = number
  let add(self)(rhs: number): number = { number { value: self.value + rhs.value } }
}
pub let make(value: i32): number = { number { value: value } }
pub let value(move number: number): i32 = { number.value }
pub let maybe(value: i32): option(i32) = { option(i32).some(value) }
"#,
    );
    workspace.write(
        "app/src/main.sc",
        r#"let main(): i32 = {
  let sum = dep.make(19) + dep.make(23)
  dep.value(sum) + (dep.maybe(0) ?? 0)
}
"#,
    );

    let app = workspace.join("app");
    let shared = salic()
        .arg("run")
        .arg(&app)
        .output()
        .expect("run two packages using the embedded core identity");
    assert_eq!(shared.status.code(), Some(42), "{}", output_text(&shared));

    workspace.write(
        "app/src/fake.sc",
        r#"pub let option(comptime t: type) = enum { some(t), none }
pub let make_option(): option(i32) = { option(i32).some(42) }

pub let add(comptime rhs: type) = trait {
  let output: type
  let add(move self)(move rhs: rhs): output
}
pub let sub(comptime rhs: type) = trait {
  let output: type
  let sub(move self)(move rhs: rhs): output
}
pub let number = struct { value: i32 }
extend(number, add(number)) {
  let output = number
  let add(move self)(move rhs: number): number = { number { value: self.value + rhs.value } }
}
extend(number, sub(number)) {
  let output = number
  let sub(move self)(move rhs: number): number = { number { value: self.value - rhs.value } }
}
pub let make_number(value: i32): number = { number { value: value } }
"#,
    );
    workspace.write(
        "app/src/main.sc",
        "let main(): i32 = { fake.make_option() ?? 0 }\n",
    );
    let fake_option = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a module type spoofing core option");
    assert_eq!(
        fake_option.status.code(),
        Some(1),
        "{}",
        output_text(&fake_option)
    );
    assert!(
        String::from_utf8_lossy(&fake_option.stderr)
            .contains("type `fake::option(i32)` does not implement `coalesce`"),
        "{}",
        output_text(&fake_option)
    );

    workspace.write(
        "app/src/main.sc",
        "let main(): i32 = { fake.make_number(20) + fake.make_number(22) }\n",
    );
    let fake_add = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a module trait spoofing core add");
    assert_eq!(
        fake_add.status.code(),
        Some(1),
        "{}",
        output_text(&fake_add)
    );
    assert!(
        String::from_utf8_lossy(&fake_add.stderr).contains("no matching `add` implementation"),
        "{}",
        output_text(&fake_add)
    );

    workspace.write(
        "app/src/main.sc",
        "let main(): i32 = { fake.make_number(44) - fake.make_number(2) }\n",
    );
    let fake_sub = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a module trait spoofing core sub");
    assert_eq!(
        fake_sub.status.code(),
        Some(1),
        "{}",
        output_text(&fake_sub)
    );
    assert!(
        String::from_utf8_lossy(&fake_sub.stderr).contains("no matching `sub` implementation"),
        "{}",
        output_text(&fake_sub)
    );

    workspace.write(
        "app/src/main.sc",
        "use root.fake as option\nlet main(): i32 = { option {} }\n",
    );
    let module_option = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a module alias falling back to core option");
    assert_eq!(
        module_option.status.code(),
        Some(1),
        "{}",
        output_text(&module_option)
    );
    assert!(
        String::from_utf8_lossy(&module_option.stderr)
            .contains("module `option` cannot be used as a value or callable"),
        "{}",
        output_text(&module_option)
    );

    workspace.write(
        "app/src/main.sc",
        r#"use root.fake as add
let number = struct { value: i32 }
extend(number, add(number)) {
  let output = i32
  let add(move self)(move rhs: number): i32 = { self.value + rhs.value }
}
let main(): i32 = { number { value: 20 } + number { value: 22 } }
"#,
    );
    let module_add = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a module alias falling back to core add");
    assert_eq!(
        module_add.status.code(),
        Some(1),
        "{}",
        output_text(&module_add)
    );
    assert!(
        String::from_utf8_lossy(&module_add.stderr)
            .contains("module `add` cannot be used as a type"),
        "{}",
        output_text(&module_add)
    );

    workspace.write(
        "app/src/main.sc",
        "use root.fake as never\nlet stop(): never = { loop {} }\nlet main(): i32 = { 42 }\n",
    );
    let module_never = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject an import alias falling back to core never");
    assert_eq!(
        module_never.status.code(),
        Some(1),
        "{}",
        output_text(&module_never)
    );
    assert!(
        String::from_utf8_lossy(&module_never.stderr)
            .contains("module `never` cannot be used as a type"),
        "{}",
        output_text(&module_never)
    );
}

#[test]
fn core_copy_identity_and_implementation_ownership_hold_across_packages() {
    let workspace = TestDirectory::new();
    workspace.write(
        "app/salicin.toml",
        r#"[package]
name = "copy-app"
version = "0.1.0"
edition = "2026"

[dependencies]
dep = { path = "../dep" }
"#,
    );
    workspace.write(
        "dep/salicin.toml",
        "[package]\nname = \"copy-dependency\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    workspace.write(
        "dep/src/lib.sc",
        r#"pub let Token = struct { value: i32 }
pub let make(value: i32): Token = { Token { value: value } }
"#,
    );
    workspace.write(
        "app/src/main.sc",
        r#"extend(dep.Token, copyable) {}
let main(): i32 = { 42 }
"#,
    );

    let app = workspace.join("app");
    let orphan = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a downstream copyable implementation for an upstream type");
    assert_eq!(orphan.status.code(), Some(1), "{}", output_text(&orphan));
    let stderr = String::from_utf8_lossy(&orphan.stderr);
    assert!(
        stderr.contains("`copyable` for") && stderr.contains("package that defines the type"),
        "{}",
        output_text(&orphan)
    );

    workspace.write(
        "dep/src/lib.sc",
        r#"pub let Token = struct { value: i32 }
extend(Token, copyable) {}
pub let make(value: i32): Token = { Token { value: value } }
pub let read(copy token: Token): i32 = { token.value }
"#,
    );
    workspace.write(
        "app/src/main.sc",
        r#"let main(): i32 = {
  let token = dep.make(42)
  let first = dep.read(token)
  if first == dep.read(token) { first } else { 0 }
}
"#,
    );

    let owner_impl = salic()
        .arg("run")
        .arg(&app)
        .output()
        .expect("use an upstream copyable implementation in a downstream package");
    assert_eq!(
        owner_impl.status.code(),
        Some(42),
        "{}",
        output_text(&owner_impl)
    );

    workspace.write("app/src/fake.sc", "pub let copyable = trait {}\n");
    workspace.write(
        "app/src/main.sc",
        r#"use root.fake.copyable as fake_copy
let local_type = struct { value: i32 }
extend(local_type, fake_copy) {}
let read(copy local: local_type): i32 = { local.value }
let main(): i32 = { read(local_type { value: 42 }) }
"#,
    );

    let alias_spoof = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject an alias of a fake copyable trait as a language marker");
    assert_eq!(
        alias_spoof.status.code(),
        Some(1),
        "{}",
        output_text(&alias_spoof)
    );
    let stderr = String::from_utf8_lossy(&alias_spoof.stderr);
    assert!(
        stderr.contains("requires `copyable`") && stderr.contains("does not implement copyable"),
        "{}",
        output_text(&alias_spoof)
    );

    workspace.write(
        "app/src/fake.sc",
        r#"pub let copyable = trait {}
pub let Token = struct { value: i32 }

extend(Token, copyable) {}

pub let make(value: i32): Token = { Token { value: value } }
pub let read(copy token: Token): i32 = { token.value }
"#,
    );
    workspace.write(
        "app/src/main.sc",
        "let main(): i32 = { fake.read(fake.make(42)) }\n",
    );

    let spoof = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a module trait spoofing core copyable semantics");
    assert_eq!(spoof.status.code(), Some(1), "{}", output_text(&spoof));
    let stderr = String::from_utf8_lossy(&spoof.stderr);
    assert!(
        stderr.contains("requires `copyable`") && stderr.contains("does not implement copyable"),
        "{}",
        output_text(&spoof)
    );
}

#[test]
fn transitive_diamond_dependencies_share_nominal_identity() {
    let workspace = TestDirectory::new();
    workspace.write(
        "shared/salicin.toml",
        "[package]\nname = \"shared-token\"\nversion = \"1.0.0\"\nedition = \"2026\"\n",
    );
    workspace.write(
        "shared/src/lib.sc",
        "pub let Token = struct { pub value: i32 }\npub let make(value: i32): Token = { Token { value: value } }\n",
    );
    for side in ["left", "right"] {
        workspace.write(
            &format!("{side}/salicin.toml"),
            &format!(
                "[package]\nname = \"{side}-side\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\nshared = {{ path = \"../shared\" }}\n"
            ),
        );
        workspace.write(
            &format!("{side}/src/lib.sc"),
            "pub use shared.Token\npub let make(value: i32): Token = { shared.make(value) }\n",
        );
    }
    workspace.write(
        "app/salicin.toml",
        r#"[package]
name = "diamond-app"
version = "0.1.0"
edition = "2026"

[dependencies]
left = { path = "../left" }
right = { path = "../right" }
"#,
    );
    workspace.write(
        "app/src/main.sc",
        r#"let bridge(move value: left.Token): right.Token = { value }
let main(): i32 = { bridge(left.make(42)).value }
"#,
    );

    let app = workspace.join("app");
    let output = salic()
        .arg("run")
        .arg(&app)
        .output()
        .expect("run diamond dependency graph");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let lock = fs::read_to_string(app.join("salicin.lock")).unwrap();
    assert_eq!(lock.matches("[[package]]").count(), 4, "{lock}");
    assert_eq!(lock.matches("name = \"shared-token\"").count(), 3, "{lock}");
}

#[test]
fn dependency_cycles_and_binary_only_dependencies_fail_before_writing_a_lockfile() {
    let cycle = TestDirectory::new();
    cycle.write(
        "app/salicin.toml",
        "[package]\nname = \"cycle-app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\nb = { path = \"../b\" }\n",
    );
    cycle.write("app/src/lib.sc", "pub let value(): i32 = { 1 }\n");
    cycle.write("app/src/main.sc", "let main(): i32 = { 0 }\n");
    cycle.write(
        "b/salicin.toml",
        "[package]\nname = \"cycle-b\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\napp = { path = \"../app\" }\n",
    );
    cycle.write("b/src/lib.sc", "pub let value(): i32 = { 2 }\n");

    let cyclic = salic()
        .arg("check")
        .arg(cycle.join("app"))
        .output()
        .expect("reject local dependency cycle");
    assert_eq!(cyclic.status.code(), Some(2), "{}", output_text(&cyclic));
    assert!(
        String::from_utf8_lossy(&cyclic.stderr)
            .to_lowercase()
            .contains("cycle"),
        "{}",
        output_text(&cyclic)
    );
    assert!(!cycle.join("app/salicin.lock").exists());

    let missing = TestDirectory::new();
    missing.write(
        "app/salicin.toml",
        "[package]\nname = \"missing-lib-app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\ntool = { path = \"../tool\" }\n",
    );
    missing.write("app/src/main.sc", "let main(): i32 = { 0 }\n");
    missing.write(
        "tool/salicin.toml",
        "[package]\nname = \"binary-tool\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    missing.write("tool/src/main.sc", "let main(): i32 = { 0 }\n");

    let no_library = salic()
        .arg("check")
        .arg(missing.join("app"))
        .output()
        .expect("reject binary-only dependency");
    assert_eq!(
        no_library.status.code(),
        Some(2),
        "{}",
        output_text(&no_library)
    );
    let stderr = String::from_utf8_lossy(&no_library.stderr).to_lowercase();
    assert!(
        stderr.contains("library") && stderr.contains("tool"),
        "{}",
        output_text(&no_library)
    );
    assert!(!missing.join("app/salicin.lock").exists());
}

#[test]
fn package_outputs_cannot_overwrite_the_manifest_or_another_target() {
    let project = TestDirectory::new();
    let manifest_text = r#"[package]
name = "protected"
version = "0.1.0"
edition = "2026"

[[bin]]
name = "protected"
path = "src/main.sc"

[[bin]]
name = "other"
path = "src/other.sc"
"#;
    let main_text = "let main(): i32 = { 0 }\n";
    let other_text = "let main(): i32 = { 1 }\n";
    let manifest = project.write("salicin.toml", manifest_text);
    project.write("src/main.sc", main_text);
    let other = project.write("src/other.sc", other_text);

    let emit = salic()
        .args(["emit-ir", "--bin", "protected"])
        .arg(&project.0)
        .arg("-o")
        .arg(&manifest)
        .output()
        .expect("reject manifest overwrite");
    assert_eq!(emit.status.code(), Some(2), "{}", output_text(&emit));
    assert_eq!(fs::read_to_string(&manifest).unwrap(), manifest_text);

    let build = salic()
        .args(["build", "--bin", "protected"])
        .arg(&project.0)
        .arg("-o")
        .arg(&other)
        .output()
        .expect("reject another target overwrite");
    assert_eq!(build.status.code(), Some(2), "{}", output_text(&build));
    assert_eq!(fs::read_to_string(&other).unwrap(), other_text);
}

#[test]
fn prelude_never_coerces_through_diverging_calls() {
    let temporary = TestDirectory::new();
    let source = temporary.write(
        "never.sc",
        r#"use core.result
let throwing = core.error.throwing
let stop(): never = { loop {} }
let absurd(move value: never): i32 = { value }
let propagate(move value: never): result(())(i32) = { value }
let raise_unit(): never with(throwing(())) = { throw(error: ())(()) }
let throw_never(): i32 with(throwing(())) = { raise_unit() }
let empty = enum {}
let holder = struct { value: empty }
let project(move holder: holder): i32 = { holder.value }
let choose(flag: bool): i32 = { if flag { 42 } else { stop() } }
let main(): i32 = { choose(true) }
"#,
    );

    let output = salic()
        .arg("run")
        .arg(source)
        .output()
        .expect("run program with never coercion");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn file_modules_resolve_flat_nested_and_nominal_members() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        r#"[package]
name = "module-app"
version = "0.1.0"
edition = "2026"
"#,
    );
    project.write(
        "src/main.sc",
        r#"let main(): i32 = {
  let reply: net.http.reply = net.http.reply()
  let status: net.http.status = net.http.status.ok(2)
  let extra = status match {
    net.http.status.ok(value) => value,
    net.http.status.err => 0
  }
  math.answer() + reply.value + extra
}
"#,
    );
    project.write(
        "src/math.sc",
        r#"pub(package) let number = struct { value: i32 }
let read = trait {
  let read(self: borrow(self))(): i32
}
extend(number, read) {
  let read(self: borrow(self))(): i32 = { self.value }
}
pub(package) let answer(): i32 = {
  let number = number { value: 40 }
  number.read()
}
"#,
    );
    project.write(
        "src/net/http.sc",
        r#"pub(package) let reply = struct { pub(package) value: i32 }
pub(package) let status = enum {
  ok(i32),
  err,
}
pub(package) let reply(): reply = { reply { value: 0 } }
"#,
    );

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .output()
        .expect("run package with file modules");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn field_visibility_controls_cross_module_and_cross_package_data_access() {
    let private_project = TestDirectory::new();
    private_project.write(
        "salicin.toml",
        "[package]\nname = \"private-fields\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    private_project.write(
        "src/data.sc",
        r#"pub(package) let Record = struct { secret: i32, pub(package) open: i32 }
pub(package) let Event = enum { Named(secret: i32), empty }
pub(package) let record(): Record = { Record { secret: 20, open: 22 } }
pub(package) let event(): Event = { Event.Named(secret: 42) }
"#,
    );
    private_project.write(
        "src/main.sc",
        r#"let read(): i32 = { data.record().secret }
let build(): data.Record = { data.Record { secret: 20, open: 22 } }
let unpack(): i32 = { data.event() match {
  data.Event.Named(secret: value) => value,
  data.Event.empty => 0,
} }
let main(): i32 = { 0 }
"#,
    );
    let denied = salic()
        .arg("check")
        .arg(&private_project.0)
        .output()
        .expect("reject private fields outside their defining module");
    assert_eq!(denied.status.code(), Some(1), "{}", output_text(&denied));
    let stderr = String::from_utf8_lossy(&denied.stderr);
    assert!(
        stderr.contains("Record.secret") && stderr.contains("Event.Named.secret"),
        "{}",
        output_text(&denied)
    );

    let workspace = TestDirectory::new();
    workspace.write(
        "dep/salicin.toml",
        "[package]\nname = \"public-fields-dep\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    workspace.write(
        "dep/src/lib.sc",
        r#"pub let Record = struct { pub value: i32 }
pub let Event = enum { Named(pub value: i32), empty }
"#,
    );
    workspace.write(
        "app/salicin.toml",
        r#"[package]
name = "public-fields-app"
version = "0.1.0"
edition = "2026"

[dependencies]
dep = { path = "../dep" }
"#,
    );
    workspace.write(
        "app/src/main.sc",
        r#"let main(): i32 = {
  let record = dep.Record { value: 20 }
  let event = dep.Event.Named(value: 22)
  let extra = event match {
    dep.Event.Named(value: value) => value,
    dep.Event.empty => 0,
  }
  record.value + extra
}
"#,
    );
    let allowed = salic()
        .arg("run")
        .arg(workspace.join("app"))
        .output()
        .expect("run with public fields across a dependency boundary");
    assert_eq!(allowed.status.code(), Some(42), "{}", output_text(&allowed));
}

#[test]
fn file_module_diagnostics_include_visibility_and_qualified_paths() {
    let private_member = TestDirectory::new();
    private_member.write(
        "salicin.toml",
        r#"[package]
name = "private-module"
version = "0.1.0"
edition = "2026"
"#,
    );
    private_member.write("src/main.sc", "let main(): i32 = { sibling.secret() }\n");
    private_member.write("src/sibling.sc", "let secret(): i32 = { 42 }\n");

    let private = salic()
        .arg("check")
        .arg(&private_member.0)
        .output()
        .expect("check private sibling access");
    assert!(
        !private.status.success(),
        "private member unexpectedly passed"
    );
    let stderr = String::from_utf8_lossy(&private.stderr).to_lowercase();
    assert!(
        stderr.contains("private") && stderr.contains("sibling") && stderr.contains("secret"),
        "{}",
        output_text(&private)
    );

    let unknown_nested_member = TestDirectory::new();
    unknown_nested_member.write(
        "salicin.toml",
        r#"[package]
name = "unknown-nested-member"
version = "0.1.0"
edition = "2026"
"#,
    );
    unknown_nested_member.write("src/main.sc", "let main(): i32 = { net.http.missing() }\n");
    unknown_nested_member.write(
        "src/net/http.sc",
        "pub(package) let answer(): i32 = { 42 }\n",
    );

    let unknown = salic()
        .arg("check")
        .arg(&unknown_nested_member.0)
        .output()
        .expect("check unknown nested module member");
    assert!(
        !unknown.status.success(),
        "unknown member unexpectedly passed"
    );
    let stderr = String::from_utf8_lossy(&unknown.stderr).to_lowercase();
    assert!(
        stderr.contains("net.http") && stderr.contains("missing"),
        "nested module path was absent from the diagnostic:\n{}",
        output_text(&unknown)
    );
}

#[test]
fn unselected_binary_targets_are_not_file_modules() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        r#"[package]
name = "separate-targets"
version = "0.1.0"
edition = "2026"

[[bin]]
name = "primary"
path = "src/main.sc"

[[bin]]
name = "tool"
path = "src/tool.sc"
"#,
    );
    project.write("src/main.sc", "let main(): i32 = { 42 }\n");
    project.write("src/tool.sc", "this is deliberately not Salicin\n");

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .args(["--bin", "primary"])
        .output()
        .expect("run one binary without compiling another target");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn file_module_paths_reject_keywords_and_the_underscore_segment() {
    for segment in ["let", "_"] {
        let project = TestDirectory::new();
        project.write(
            "salicin.toml",
            "[package]\nname = \"reserved-module\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
        );
        project.write("src/main.sc", "let main(): i32 = { 42 }\n");
        project.write(&format!("src/{segment}.sc"), "let value = 0\n");

        let output = salic()
            .arg("check")
            .arg(&project.0)
            .output()
            .expect("reject an unspellable file-module path");
        assert_eq!(output.status.code(), Some(2), "{}", output_text(&output));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(segment) && stderr.contains("reserved"),
            "{}",
            output_text(&output)
        );
    }
}

#[test]
fn private_use_supports_groups_aliases_and_relative_module_roots() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        r#"[package]
name = "use-paths"
version = "0.1.0"
edition = "2026"
"#,
    );
    project.write(
        "src/main.sc",
        r#"let root_bonus(): i32 = { 3 }
let main(): i32 = { nested.deep.answer() }
"#,
    );
    project.write(
        "src/kit.sc",
        r#"pub(package) let number = struct { pub(package) value: i32 }
pub(package) let outcome = enum {
  ready(i32),
  empty,
}
pub(package) let zero(): i32 = { 0 }
pub(package) let increment(value: i32): i32 = { value + 1 }
pub(package) let make_number(value: i32): number = { number { value: value } }
"#,
    );
    project.write("src/nested.sc", "let parent_bonus(): i32 = { 2 }\n");
    project.write(
        "src/nested/deep.sc",
        r#"use root.kit.{number, outcome, increment}
let make = root.kit.make_number
let utilities = root.kit
let local = self.local_bonus
let parent = super.parent_bonus
let from_root = root.root_bonus

let local_bonus(): i32 = { 1 }

pub(package) let answer(): i32 = {
  let number: number = make(35)
  let outcome: outcome = outcome.ready(increment(number.value))
  let value = outcome match {
    outcome.ready(value) => value,
    outcome.empty => 0
  }
  value + utilities.zero() + local() + parent() + from_root()
}
"#,
    );

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .output()
        .expect("run project with private and relative imports");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn public_and_package_use_build_facade_modules() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        r#"[package]
name = "use-facades"
version = "0.1.0"
edition = "2026"
"#,
    );
    project.write(
        "src/main.sc",
        "let main(): i32 = { facade.answer() + package_facade.extra() }\n",
    );
    project.write("src/implementation.sc", "pub let answer(): i32 = { 40 }\n");
    project.write(
        "src/package_implementation.sc",
        "pub(package) let extra(): i32 = { 2 }\n",
    );
    project.write("src/facade.sc", "pub use root.implementation.answer\n");
    project.write(
        "src/package_facade.sc",
        "pub(package) use root.package_implementation.extra\n",
    );

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .output()
        .expect("run project through public facade imports");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn local_bindings_shadow_imports_without_hiding_them_from_outer_scopes() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        r#"[package]
name = "use-shadowing"
version = "0.1.0"
edition = "2026"
"#,
    );
    project.write(
        "src/main.sc",
        r#"use root.numbers.answer

let main(): i32 = {
  let imported = answer()
  let local = do {
    let answer = 2
    answer
  }
  imported + local
}
"#,
    );
    project.write(
        "src/numbers.sc",
        "pub(package) let answer(): i32 = { 40 }\n",
    );

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .output()
        .expect("run project where a local shadows an import");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn invalid_imports_report_alias_paths_and_visibility() {
    struct Case {
        name: &'static str,
        root: &'static str,
        modules: &'static [(&'static str, &'static str)],
        expected: &'static [&'static str],
    }

    let cases = [
        Case {
            name: "duplicate-alias",
            root: r#"use root.first.answer as selected
let selected = root.second.answer
let main(): i32 = { selected() }
"#,
            modules: &[
                ("src/first.sc", "pub(package) let answer(): i32 = { 1 }\n"),
                ("src/second.sc", "pub(package) let answer(): i32 = { 2 }\n"),
            ],
            expected: &["duplicate", "selected", "first.answer", "second.answer"],
        },
        Case {
            name: "unknown-import",
            root: "use root.net.missing as answer\nlet main(): i32 = { answer() }\n",
            modules: &[("src/net.sc", "pub(package) let present(): i32 = { 42 }\n")],
            expected: &["unknown", "net.missing"],
        },
        Case {
            name: "private-sibling-import",
            root: "use root.sibling.secret\nlet main(): i32 = { secret() }\n",
            modules: &[("src/sibling.sc", "let secret(): i32 = { 42 }\n")],
            expected: &["private", "sibling.secret"],
        },
        Case {
            name: "public-private-promotion",
            root: "let main(): i32 = { 0 }\n",
            modules: &[(
                "src/facade.sc",
                "let secret(): i32 = { 1 }\npub use self.secret as exposed\n",
            )],
            expected: &["pub use", "private", "facade.secret"],
        },
        Case {
            name: "public-package-promotion",
            root: "let main(): i32 = { 0 }\n",
            modules: &[(
                "src/facade.sc",
                "pub(package) let internal(): i32 = { 1 }\npub use self.internal as exposed\n",
            )],
            expected: &["pub use", "pub(package)", "facade.internal"],
        },
        Case {
            name: "private-module-alias",
            root: "let main(): i32 = { 0 }\n",
            modules: &[
                ("src/secret.sc", "pub let open(): i32 = { 1 }\n"),
                ("src/a.sc", "use root.secret as hidden\n"),
                ("src/b.sc", "use root.a.hidden.open as leak\n"),
            ],
            expected: &["private", "a.hidden"],
        },
    ];

    for case in cases {
        let project = TestDirectory::new();
        project.write(
            "salicin.toml",
            &format!(
                "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
                case.name
            ),
        );
        project.write("src/main.sc", case.root);
        for (path, source) in case.modules {
            project.write(path, source);
        }

        let output = salic()
            .arg("check")
            .arg(&project.0)
            .output()
            .expect("check invalid import project");
        assert!(
            !output.status.success(),
            "{} unexpectedly passed",
            case.name
        );
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        for expected in case.expected {
            assert!(
                stderr.contains(expected),
                "{} did not report `{expected}`:\n{}",
                case.name,
                output_text(&output)
            );
        }
    }
}
