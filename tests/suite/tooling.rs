use crate::support::*;

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
        "test(\"dependency test must not run\") { false }\n",
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
    workspace.write(
        "app/src/main.sc",
        "test(\"primary package test\") { true }\n",
    );
    let package = salic()
        .arg("test")
        .arg(app)
        .output()
        .expect("run primary package tests");
    assert_eq!(package.status.code(), Some(0), "{}", output_text(&package));
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
            "type mismatch for array index: expected `i32`, found `bool`",
        ),
        (
            "throw_in_plain_return.sc",
            2,
            3,
            "call to `throw` requires `throws(bool)`; handle it with `try { ... }` or propagate it from the current function",
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
        "standard_throw_ambiguous_throws.sc",
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
fn inventory_example_exercises_lib1_across_modules() {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/inventory");

    let checked = salic()
        .arg("check")
        .arg(&package)
        .output()
        .expect("check LIB1 inventory example");
    assert!(checked.status.success(), "{}", output_text(&checked));

    let output = salic()
        .arg("run")
        .arg(&package)
        .output()
        .expect("run LIB1 inventory example");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}
