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
fn structured_test_failures_report_all_messages_and_reject_unhandled_effects() {
    let temporary = TestDirectory::new();
    let source = temporary.write(
        "structured.sc",
        "test(\"messaged\") {\n\
           core.testing.failure.abort(core.option.some(\"盐: expected 42\"))\n\
         }\n\
         test(\"boolean migration\") { false }\n\
         test(\"empty message\") {\n\
           core.testing.failure.abort(core.option.some(\"\"))\n\
         }\n\
         test(\"later registration\") {\n\
           core.testing.failure.abort(core.option.some(\"later still ran\"))\n\
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
         salic: test \"boolean migration\" failed\n\
         salic: test \"empty message\" failed: \n\
         salic: test \"later registration\" failed: later still ran\n\
         salic: test result: 0 passed; 4 failed; 4 selected\n",
        "{}",
        output_text(&output)
    );

    let unrelated = temporary.write(
        "unrelated.sc",
        "let unrelated = effect { let escape(): () }\n\
         test(\"wrong effect\") {\n\
           unrelated.escape()\n\
           true\n\
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
         let common_assertions_pass: with(core.testing.failure)(): bool = {\n\
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
           some + ok == 42 && error == 7 && counter == 2\n\
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
        "let fail_assert: with(core.testing.failure)(): bool = {\n\
           std.test.assert(false)\n\
           true\n\
         }\n\
         let fail_assert_eq: with(core.testing.failure)(): bool = {\n\
           let left: i64 = 1\n\
           let right: i64 = 2\n\
           std.test.assert_eq(i64)(left)(right)\n\
           true\n\
         }\n\
         let fail_assert_ne: with(core.testing.failure)(): bool = {\n\
           let value: i64 = 7\n\
           std.test.assert_ne(i64)(value)(value)\n\
           true\n\
         }\n\
         let fail_expect_some: with(core.testing.failure)(): bool = {\n\
           let value: core.option(i64) = core.option.none\n\
           std.test.expect_some(i64)(value) == 0\n\
         }\n\
         let fail_expect_none: with(core.testing.failure)(): bool = {\n\
           let value: core.option(i64) = core.option.some(9)\n\
           std.test.expect_none(i64)(value)\n\
           true\n\
         }\n\
         let fail_expect_ok: with(core.testing.failure)(): bool = {\n\
           let value: core.result(i64)(i64) = core.result.err(0)\n\
           std.test.expect_ok(i64, i64)(value) == 0\n\
         }\n\
         let fail_expect_err: with(core.testing.failure)(): bool = {\n\
           let value: core.result(i64)(i64) = core.result.ok(11)\n\
           std.test.expect_err(i64, i64)(value) == 0\n\
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
        "test(\"zeta\") { false }\n\
         test(\"alpha\") { true }\n\
         test(\"alphabet\") { false }\n",
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
        "salic: test \"alphabet\" failed\n\
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
        "salic: test \"zeta\" failed\n\
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
        "test(\"same name\") { true }\n\
         test(\"same name\") { true }\n",
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
    duplicate.write("src/main.sc", "test(\"same name\") { true }\n");
    duplicate.write("src/other.sc", "test(\"same name\") { true }\n");
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
             core.testing.failure.abort(core.option.none)\n\
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
