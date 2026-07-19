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
