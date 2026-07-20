use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn salic() -> Command {
    Command::new(env!("CARGO_BIN_EXE_salic"))
}

fn fixture(kind: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(kind)
        .join(name)
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
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

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn create_dir(&self, name: &str) -> PathBuf {
        let path = self.join(name);
        fs::create_dir_all(&path).expect("create nested test directory");
        path
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
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
fn emit_ir_and_check_cover_the_frontend() {
    let emitted = salic()
        .args(["emit-ir"])
        .arg(fixture("pass", "exit_42.sali"))
        .output()
        .expect("emit LLVM IR");
    assert!(emitted.status.success(), "{}", output_text(&emitted));
    let ir = String::from_utf8_lossy(&emitted.stdout);
    assert!(ir.contains("define i32 @main()"), "unexpected IR:\n{ir}");

    let checked = salic()
        .arg("check")
        .arg(fixture("pass", "condition.sali"))
        .output()
        .expect("check source");
    assert!(checked.status.success(), "{}", output_text(&checked));

    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/basics.sali");
    let checked_example = salic()
        .arg("check")
        .arg(example)
        .output()
        .expect("check documented example");
    assert!(
        checked_example.status.success(),
        "{}",
        output_text(&checked_example)
    );
}

#[test]
fn run_supports_grouped_calls_and_unit_main() {
    let curried = salic()
        .arg("run")
        .arg(fixture("pass", "curried_call.sali"))
        .output()
        .expect("run curried program");
    assert_eq!(curried.status.code(), Some(42), "{}", output_text(&curried));

    let unit = salic()
        .arg("run")
        .arg(fixture("pass", "unit_main.sali"))
        .output()
        .expect("run unit program");
    assert!(unit.status.success(), "{}", output_text(&unit));

    let unit_values = salic()
        .arg("run")
        .arg(fixture("pass", "unit_values.sali"))
        .output()
        .expect("run program with unit values");
    assert_eq!(
        unit_values.status.code(),
        Some(42),
        "{}",
        output_text(&unit_values)
    );

    let control_flow = salic()
        .arg("run")
        .arg(fixture("pass", "short_circuit_return.sali"))
        .output()
        .expect("run short-circuit control flow program");
    assert_eq!(
        control_flow.status.code(),
        Some(42),
        "{}",
        output_text(&control_flow)
    );
}

#[test]
fn m1_struct_programs_run_with_expected_result() {
    for name in [
        "struct_fields.sali",
        "struct_mutation.sali",
        "positional_constructor.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M1 struct fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_struct_errors_report_their_cause() {
    for (name, expected) in [
        ("unknown_field.sali", "unknown field"),
        ("constructor_missing_field.sali", "missing field"),
        ("constructor_duplicate_field.sali", "duplicate field"),
        ("constructor_mixed_arguments.sali", "mixed"),
        ("immutable_field_assignment.sali", "immutable"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 struct fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_match_and_partial_programs_run_with_expected_result() {
    for name in [
        "enum_match.sali",
        "nested_match.sali",
        "match_guard.sali",
        "partial_application.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M1 match or partial-application fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_match_and_partial_errors_report_their_cause() {
    for (name, expected) in [
        ("non_exhaustive_match.sali", "exhaustive"),
        ("pattern_type_mismatch.sali", "pattern"),
        ("partial_application_escape.sali", "escape"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 match or partial-application fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_ownership_programs_run_with_expected_result() {
    for name in [
        "shared_borrow_call.sali",
        "mut_borrow_field_update.sali",
        "explicit_move_i32_once.sali",
        "borrow_released_after_complete_call.sali",
        "borrowed_unit_is_abi_erased.sali",
        "branch_move_does_not_pollute_sibling.sali",
        "disjoint_mut_field_borrows.sali",
        "inferred_copy_i32.sali",
        "move_then_return_preserves_other_branch.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M1 ownership fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_ownership_errors_report_their_cause() {
    for (name, expected) in [
        ("use_after_move.sali", &["moved"][..]),
        ("use_after_explicit_move_i32.sali", &["moved"][..]),
        (
            "copy_non_copy.sali",
            &["requires `Copy`", "does not implement Copy"][..],
        ),
        (
            "double_mut_borrow.sali",
            &["mutable borrow", "already borrowed"][..],
        ),
        ("borrow_move_conflict.sali", &["move", "borrowed"][..]),
        (
            "same_field_mut_borrow_conflict.sali",
            &["mutable borrow", "already borrowed"][..],
        ),
        ("use_after_inferred_move.sali", &["moved"][..]),
        ("possibly_moved_after_branch.sali", &["possibly moved"][..]),
        ("both_branches_move.sali", &["moved"][..]),
        ("short_circuit_possibly_moves.sali", &["possibly moved"][..]),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 ownership fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        for fragment in expected {
            assert!(
                stderr.contains(fragment),
                "{name} did not report `{fragment}`:\n{}",
                output_text(&output)
            );
        }
        assert!(
            !stderr.contains("not supported"),
            "{name} reached a placeholder diagnostic:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_local_closure_programs_run_with_expected_result() {
    for name in [
        "capturing_closure.sali",
        "closure_shared_repeat.sali",
        "closure_capture_parameter.sali",
        "closure_curried_capture.sali",
        "closure_mut_capture.sali",
        "closure_move_once.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M1 closure fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_local_closure_errors_report_their_cause() {
    for (name, expected) in [
        ("closure_escape_return.sali", "escape"),
        ("closure_partial_application.sali", "partial application"),
        ("closure_fnmut_immutable.sali", "FnMut"),
        ("closure_capture_borrow_conflict.sali", "borrowed"),
        ("closure_fnonce_twice.sali", "consumed"),
        ("closure_move_capture_source_use.sali", "moved"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 closure fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn shorthand_builds_a_native_executable() {
    let temporary = TestDirectory::new();
    let executable = temporary.join("mutation");
    let built = salic()
        .arg(fixture("pass", "block_mutation.sali"))
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("build source");
    assert!(built.status.success(), "{}", output_text(&built));
    assert!(executable.is_file());

    let status = Command::new(executable)
        .status()
        .expect("run native executable");
    assert_eq!(status.code(), Some(42));
}

#[test]
fn source_errors_fail_check_without_creating_output() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fail");
    let mut fixtures: Vec<_> = fs::read_dir(directory)
        .expect("read failure fixtures")
        .map(|entry| entry.expect("read directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sali")
        })
        .collect();
    fixtures.sort();

    for path in fixtures {
        let name = path.file_name().unwrap().to_string_lossy();
        let output = salic()
            .arg("check")
            .arg(&path)
            .output()
            .expect("check invalid source");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            !output.stderr.is_empty(),
            "{name} produced no diagnostic output"
        );
    }
}

#[test]
fn every_pass_fixture_checks_successfully() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pass");
    let mut fixtures: Vec<_> = fs::read_dir(directory)
        .expect("read passing fixtures")
        .map(|entry| entry.expect("read directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sali")
        })
        .collect();
    fixtures.sort();

    for path in fixtures {
        let output = salic()
            .arg("check")
            .arg(&path)
            .output()
            .expect("check valid source");
        assert!(
            output.status.success(),
            "{} failed:\n{}",
            path.display(),
            output_text(&output)
        );
    }
}

#[test]
fn m1_loops_and_arrays_run_with_expected_result() {
    for name in [
        "while_mutation.sali",
        "loop_break_value.sali",
        "fixed_array_index.sali",
        "dynamic_array_index.sali",
        "empty_array_typed.sali",
        "nested_loop_break.sali",
        "loop_move_then_break.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M1 loop or array fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_array_errors_report_their_cause() {
    for (name, expected) in [
        ("array_index_type.sali", "index"),
        ("array_length_mismatch.sali", "length"),
        ("array_constant_oob.sali", "out of bounds"),
        ("array_negative_oob.sali", "out of bounds"),
        ("array_empty_without_context.sali", "empty array"),
        ("array_non_copy_element.sali", "Copy"),
        ("array_index_assignment.sali", "indexed"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 array fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_loop_errors_report_their_cause() {
    for (name, expected) in [
        ("break_outside_loop.sali", "outside"),
        ("while_break_value.sali", "while"),
        ("loop_break_type_mismatch.sali", "type mismatch"),
        ("loop_backedge_move.sali", "move"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 loop fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn dynamic_array_out_of_bounds_traps() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "dynamic_array_oob.sali"))
        .output()
        .expect("run dynamically out-of-bounds array fixture");
    assert!(
        !output.status.success(),
        "out-of-bounds indexing unexpectedly succeeded:\n{}",
        output_text(&output)
    );
}

#[test]
fn m1_inherent_members_run_with_expected_result() {
    for name in [
        "inherent_reset_and_constant.sali",
        "inherent_grouped_shared_method.sali",
        "inherent_move_receiver.sali",
        "inherent_associated_function.sali",
        "inherent_associated_field_same_name.sali",
        "inherent_method_and_associated_same_name.sali",
        "inherent_local_shadows_type.sali",
        "inherent_recursive_method.sali",
        "inherent_enum_method.sali",
        "inherent_receiver_loan_released.sali",
        "inherent_disjoint_forward_extend.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M1 inherent-member fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_inherent_member_errors_report_their_cause() {
    for (name, expected) in [
        (
            "inherent_field_method_conflict.sali",
            "conflicts with field",
        ),
        (
            "inherent_duplicate_method.sali",
            "duplicate inherent method",
        ),
        (
            "inherent_duplicate_associated.sali",
            "duplicate associated member",
        ),
        (
            "inherent_variant_associated_conflict.sali",
            "conflicts with variant",
        ),
        ("inherent_mut_receiver_immutable.sali", "immutable"),
        ("inherent_unknown_target.sali", "unknown extension target"),
        ("inherent_trait_extension_pending.sali", "unknown trait"),
        ("inherent_bound_method_value.sali", "must be called"),
        ("inherent_associated_function_value.sali", "must be called"),
        (
            "inherent_temporary_borrow_receiver.sali",
            "temporary receiver",
        ),
        ("inherent_move_receiver_reuse.sali", "moved"),
        ("inherent_borrowed_partial.sali", "partial application"),
        ("inherent_receiver_borrow_conflict.sali", "borrowed"),
        ("inherent_non_nominal_target.sali", "nominal"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 inherent-member fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_generic_function_programs_run_with_expected_result() {
    for name in [
        "generic_identity.sali",
        "generic_multiple_instances.sali",
        "generic_type_application_partial.sali",
        "generic_composition.sali",
        "generic_same_instance_recursion.sali",
        "generic_call_inside_closure.sali",
        "generic_validation_rollback.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M2 generic-function fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_generic_function_errors_report_their_cause() {
    for (name, expected) in [
        ("generic_unused_invalid_body.sali", "type mismatch"),
        ("generic_parameter_moved_twice.sali", "moved"),
        ("generic_missing_return_type.sali", "return type"),
        ("generic_unconstrained_member.sali", "generic parameter"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M2 generic-function fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_generic_nominal_programs_run_with_expected_result() {
    for name in [
        "generic_struct.sali",
        "generic_nested_struct.sali",
        "generic_enum_match.sali",
        "generic_function_constructs_nominal.sali",
        "generic_nominal_multiple_instances.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M2 generic-nominal fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_generic_nominal_errors_report_their_cause() {
    for (name, expected) in [
        ("generic_nominal_unknown_field_type.sali", "unknown type"),
        ("generic_nominal_recursive_layout.sali", "infinite size"),
        ("generic_nominal_argument_count.sali", "argument count"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M2 generic-nominal fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_inferred_type_arguments_run_with_expected_result() {
    for name in [
        "infer_generic_function.sali",
        "infer_function_from_expected.sali",
        "infer_generic_struct.sali",
        "infer_nested_generic_struct.sali",
        "infer_nominal_from_expected.sali",
        "infer_generic_enum_variant.sali",
        "infer_runtime_partial.sali",
        "infer_argument_once.sali",
        "infer_constraint_order.sali",
        "infer_fresh_constructor.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run inferred-type-argument fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_inferred_type_argument_errors_report_their_cause() {
    for (name, expected) in [
        ("infer_conflicting_arguments.sali", "conflicting"),
        ("infer_expected_conflict.sali", "conflicting"),
        ("infer_unconstrained.sali", "cannot infer"),
        ("infer_incomplete_application.sali", "cannot infer"),
        ("infer_unsupported_probe.sali", "explicit type argument"),
        ("infer_nested_hole.sali", "nested"),
        ("infer_moved_argument.sali", "moved"),
        ("infer_borrow_temporary.sali", "place"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid inferred-type-argument fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_concrete_trait_programs_run_with_expected_result() {
    for name in [
        "trait_unique_method.sali",
        "trait_associated_output.sali",
        "trait_generic_nominal_impl.sali",
        "trait_inherent_precedence.sali",
        "trait_declaration_order.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run concrete-trait fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_concrete_trait_errors_report_their_cause() {
    for (name, expected) in [
        ("trait_unknown_trait.sali", "unknown trait"),
        (
            "trait_duplicate_impl.sali",
            "duplicate trait implementation",
        ),
        ("trait_missing_method.sali", "missing trait method"),
        ("trait_missing_type.sali", "missing associated type"),
        ("trait_extra_member.sali", "unknown trait member"),
        ("trait_pass_mode_mismatch.sali", "signature mismatch"),
        ("trait_group_mismatch.sali", "signature mismatch"),
        ("trait_return_mismatch.sali", "signature mismatch"),
        ("trait_ambiguous_method.sali", "ambiguous trait method"),
        (
            "trait_generic_impl_pending.sali",
            "generic trait implementation",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid concrete-trait fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_add_trait_programs_run_with_expected_result() {
    for name in [
        "add_trait_nominal_pair.sali",
        "add_trait_nominal_i32_nominal_output.sali",
        "add_trait_nominal_i32_scalar_output.sali",
        "add_trait_builtin_integer_precedence.sali",
        "add_trait_operands_once.sali",
        "add_trait_expected_output.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run Add-trait fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_add_trait_errors_report_their_cause() {
    for (name, expected) in [
        ("add_trait_missing_impl.sali", "Add"),
        ("add_trait_rhs_mismatch.sali", "Add"),
        ("add_trait_ambiguous_literal.sali", "ambiguous"),
        ("add_trait_use_after_move.sali", "moved"),
        ("add_trait_rhs_use_after_move.sali", "moved"),
        ("add_trait_malformed_schema.sali", "Add"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid Add-trait fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_option_and_result_prelude_programs_run_with_expected_result() {
    for name in [
        "prelude_option_some.sali",
        "prelude_option_none.sali",
        "prelude_result_ok.sali",
        "prelude_result_err.sali",
        "prelude_nested_option_result.sali",
        "prelude_multiple_instances.sali",
        "prelude_inferred_variants.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run Option/Result prelude fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_option_and_result_prelude_errors_report_their_cause() {
    for (name, expected) in [
        ("prelude_redefine_option.sali", "Option"),
        ("prelude_redefine_result.sali", "Result"),
        ("prelude_option_arity.sali", "argument count"),
        ("prelude_result_arity.sali", "argument count"),
        ("prelude_option_payload_mismatch.sali", "type mismatch"),
        ("prelude_result_ok_payload_mismatch.sali", "type mismatch"),
        ("prelude_result_err_payload_mismatch.sali", "type mismatch"),
        ("prelude_option_expected_mismatch.sali", "type mismatch"),
        ("prelude_result_expected_mismatch.sali", "type mismatch"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid Option/Result prelude fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_coalesce_programs_run_with_expected_result() {
    for name in [
        "coalesce_option_some_short_circuit.sali",
        "coalesce_option_none_fallback.sali",
        "coalesce_result_ok_short_circuit.sali",
        "coalesce_result_err_fallback.sali",
        "coalesce_right_associative.sali",
        "coalesce_logical_or_precedence.sali",
        "coalesce_match_precedence_nested_option.sali",
        "coalesce_lhs_once.sali",
        "coalesce_nested_result_payload.sali",
        "coalesce_infer_option_none.sali",
        "coalesce_infer_result_err.sali",
        "coalesce_infer_right_associative_none.sali",
        "coalesce_infer_local_without_annotation.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run null-coalescing fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_coalesce_errors_report_their_cause() {
    for (name, expected) in [
        ("coalesce_option_use_after_move.sali", "moved"),
        ("coalesce_result_use_after_move.sali", "moved"),
        ("coalesce_option_rhs_mismatch.sali", "type mismatch"),
        ("coalesce_result_rhs_mismatch.sali", "type mismatch"),
        ("coalesce_non_container_lhs.sali", "Option"),
        (
            "coalesce_infer_result_error_unconstrained.sali",
            "cannot infer",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid null-coalescing fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_try_programs_run_with_expected_result() {
    for name in [
        "try_option_some_continue.sali",
        "try_option_none_propagate.sali",
        "try_result_ok_continue.sali",
        "try_result_err_propagate.sali",
        "try_result_success_type_changes.sali",
        "try_auto_wrap_tail.sali",
        "try_auto_wrap_return.sali",
        "try_auto_wrap_shadowing.sali",
        "try_full_container_unchanged.sali",
        "try_inferred_operand.sali",
        "try_nested_auto_wrap.sali",
        "try_unit_success.sali",
        "try_then_member.sali",
        "try_operand_once.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run try-propagation fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_try_errors_report_their_cause() {
    for (name, expected) in [
        ("try_non_container_return.sali", "return"),
        ("try_in_global.sali", "global"),
        ("try_omitted_return_type.sali", "return type"),
        ("try_in_closure.sali", "closure"),
        ("try_option_into_result.sali", "Option"),
        ("try_result_into_option.sali", "Result"),
        ("try_result_error_mismatch.sali", "error type"),
        ("try_non_container_operand.sali", "Option"),
        ("try_use_after_move.sali", "moved"),
        ("try_auto_wrap_type_mismatch.sali", "type mismatch"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid try-propagation fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_optional_chain_programs_run_with_expected_result() {
    for name in [
        "chain_option_some_field.sali",
        "chain_option_none_field.sali",
        "chain_result_ok_field.sali",
        "chain_result_err_field.sali",
        "chain_success_type_changes.sali",
        "chain_consecutive_fields.sali",
        "chain_option_method.sali",
        "chain_result_method.sali",
        "chain_borrowed_method.sali",
        "chain_option_method_arguments_are_lazy.sali",
        "chain_result_method_arguments_are_lazy.sali",
        "chain_inferred_inputs.sali",
        "chain_lhs_once.sali",
        "chain_method_result_is_nested.sali",
        "chain_then_coalesce.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run optional-chain fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_optional_chain_errors_report_their_cause() {
    for (name, expected) in [
        ("chain_non_container.sali", "Option"),
        ("chain_unknown_field.sali", "missing"),
        ("chain_unknown_method.sali", "missing"),
        ("chain_mut_borrow_method.sali", "mutable-borrow"),
        ("chain_method_partial_application.sali", "fully applied"),
        ("chain_use_after_move.sali", "moved"),
        ("chain_nested_result_not_flattened.sali", "type mismatch"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid optional-chain fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_throw_programs_run_with_expected_result() {
    for name in [
        "throw_result_err_propagate.sali",
        "throw_error_once.sali",
        "throw_if_flow.sali",
        "throw_generic_error.sali",
        "throw_unit_error.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run throw-propagation fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_throw_errors_report_their_cause() {
    for (name, expected) in [
        ("throw_in_option_return.sali", "Result"),
        ("throw_in_plain_return.sali", "Result"),
        ("throw_in_global.sali", "global"),
        ("throw_in_closure.sali", "closure"),
        ("throw_omitted_return_type.sali", "return type"),
        ("throw_error_type_mismatch.sali", "expected"),
        ("throw_without_value.sali", "expression"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid throw-propagation fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn output_must_not_overwrite_the_source() {
    let temporary = TestDirectory::new();
    let source = temporary.join("keep.sali");
    let original = b"let main(): i32 = 0\n";
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
    project.write("src/main.sali", "let main(): i32 = 42\n");

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
path = "src/toolbox.sali"

[[bin]]
name = "toolbox"
path = "src/main.sali"

[[bin]]
name = "answer"
path = "src/answer.sali"
"#,
    );
    project.write("src/toolbox.sali", "let answer(): i32 = 42\n");
    project.write("src/main.sali", "let main(): i32 = 1\n");
    project.write("src/answer.sali", "let main(): i32 = 42\n");

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
path = "src/left.sali"

[[bin]]
name = "right"
path = "src/right.sali"
"#,
    );
    multiple_bins.write("src/left.sali", "let main(): i32 = 1\n");
    multiple_bins.write("src/right.sali", "let main(): i32 = 2\n");

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

    let library_only = TestDirectory::new();
    library_only.write(
        "salicin.toml",
        r#"[package]
name = "library-only"
version = "0.1.0"
edition = "2026"

[lib]
path = "src/lib.sali"
"#,
    );
    library_only.write("src/lib.sali", "let answer(): i32 = 42\n");

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
    invalid_dependency.write("src/main.sali", "let main(): i32 = 0\n");

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
fn package_outputs_cannot_overwrite_the_manifest_or_another_target() {
    let project = TestDirectory::new();
    let manifest_text = r#"[package]
name = "protected"
version = "0.1.0"
edition = "2026"

[[bin]]
name = "protected"
path = "src/main.sali"

[[bin]]
name = "other"
path = "src/other.sali"
"#;
    let main_text = "let main(): i32 = 0\n";
    let other_text = "let main(): i32 = 1\n";
    let manifest = project.write("salicin.toml", manifest_text);
    project.write("src/main.sali", main_text);
    let other = project.write("src/other.sali", other_text);

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
        "never.sali",
        r#"let stop(): never = loop {}
let absurd(move value: never): i32 = value
let propagate(move value: never): Result(i32, ()) = value
let throw_never(move value: never): Result(i32, ()) = { throw value }
let Empty = enum {}
let Holder = struct(value: Empty)
let project(move holder: Holder): i32 = holder.value
let choose(flag: bool): i32 = if flag { 42 } else { stop() }
let main(): i32 = choose(true)
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
        "src/main.sali",
        r#"let main(): i32 = {
  let reply: net.http.Reply = net.http.reply()
  let status: net.http.Status = net.http.Status.Ok(2)
  let extra = status match {
    net.http.Status.Ok(value) => value,
    net.http.Status.Err => 0
  }
  math.answer() + reply.value + extra
}
"#,
    );
    project.write(
        "src/math.sali",
        r#"pub(package) let Number = struct(value: i32)
let Read = trait {
  let read(borrow self)(): i32
}
extend Number: Read {
  let read(borrow self)(): i32 = self.value
}
pub(package) let answer(): i32 = {
  let number = Number(value: 40)
  number.read()
}
"#,
    );
    project.write(
        "src/net/http.sali",
        r#"pub(package) let Reply = struct(value: i32)
pub(package) let Status = enum {
  Ok(i32),
  Err,
}
pub(package) let reply(): Reply = Reply(value: 0)
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
    private_member.write("src/main.sali", "let main(): i32 = sibling.secret()\n");
    private_member.write("src/sibling.sali", "let secret(): i32 = 42\n");

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
    unknown_nested_member.write("src/main.sali", "let main(): i32 = net.http.missing()\n");
    unknown_nested_member.write("src/net/http.sali", "pub(package) let answer(): i32 = 42\n");

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
path = "src/main.sali"

[[bin]]
name = "tool"
path = "src/tool.sali"
"#,
    );
    project.write("src/main.sali", "let main(): i32 = 42\n");
    project.write("src/tool.sali", "this is deliberately not Salicin\n");

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .args(["--bin", "primary"])
        .output()
        .expect("run one binary without compiling another target");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn file_module_paths_reject_keywords_and_the_inference_placeholder() {
    for segment in ["let", "_"] {
        let project = TestDirectory::new();
        project.write(
            "salicin.toml",
            "[package]\nname = \"reserved-module\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
        );
        project.write("src/main.sali", "let main(): i32 = 42\n");
        project.write(&format!("src/{segment}.sali"), "let value = 0\n");

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
