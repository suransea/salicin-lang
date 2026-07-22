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
fn return_type_effect_groups_run_with_expected_result() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "return_type_effects.sc"))
        .output()
        .expect("run return-type effect fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn effect_generics_select_pure_and_unsafe_instances() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "effect_generic.sc"))
        .output()
        .expect("run effect-generic fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "effect_generic_unhandled.sc"))
        .output()
        .expect("reject an unhandled selected unsafe effect");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires an `unsafe` handler"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn algebraic_effect_handlers_resume_or_abort_one_shot_continuations() {
    for fixture_name in [
        "algebraic_effect_handler.sc",
        "effect_callable_contract.sc",
        "algebraic_effect_abort.sc",
        "algebraic_effect_function_propagation.sc",
        "algebraic_effect_function_alias.sc",
        "algebraic_effect_static_higher_order.sc",
        "algebraic_effect_reusable_handler.sc",
        "algebraic_effect_capturing_closure.sc",
        "algebraic_effect_capturing_closure_drop.sc",
        "algebraic_effect_fn_mut_closure.sc",
        "algebraic_effect_dynamic_callable.sc",
        "algebraic_effect_dynamic_fn_mut_closure.sc",
        "algebraic_effect_dynamic_fn_once_drop.sc",
        "algebraic_effect_dynamic_callable_alias.sc",
        "algebraic_effect_dynamic_callable_assignment.sc",
        "algebraic_effect_dynamic_assignment_drop.sc",
        "algebraic_effect_dynamic_callable_union.sc",
        "algebraic_effect_dynamic_union_fn_mut.sc",
        "algebraic_effect_dynamic_union_drop.sc",
        "algebraic_effect_noncopy_wildcard_guard.sc",
        "algebraic_effect_noncopy_binding_guard.sc",
        "algebraic_effect_copy_binding_guard.sc",
        "algebraic_effect_noncopy_projection_guard.sc",
        "algebraic_effect_residual_effects.sc",
        "algebraic_effect_call_arguments.sc",
        "algebraic_effect_done.sc",
        "algebraic_effect_nearest_handler.sc",
        "algebraic_effect_explicit_return.sc",
        "algebraic_effect_borrow_parameters.sc",
        "algebraic_effect_post_resume.sc",
        "algebraic_effect_expression_traversal.sc",
        "algebraic_effect_short_circuit.sc",
        "algebraic_effect_coalesce.sc",
        "algebraic_effect_match_guard.sc",
        "algebraic_effect_optional_call.sc",
        "algebraic_effect_cross_function_answer.sc",
        "algebraic_effect_composition.sc",
        "algebraic_effect_recursion.sc",
        "algebraic_effect_repeated_call.sc",
        "algebraic_effect_named_overload.sc",
        "algebraic_effect_mutual_recursion.sc",
        "algebraic_effect_mutual_answer.sc",
        "algebraic_effect_loops.sc",
        "algebraic_effect_cross_function_abort.sc",
        "algebraic_effect_continuation_drop.sc",
        "algebraic_effect_continuation_resume_drop.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run algebraic-effect handler fixture");
        assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "algebraic_effect_resume_twice.sc"))
        .output()
        .expect("reject a continuation resumed twice");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("one-shot"),
        "{}",
        output_text(&output)
    );

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "algebraic_effect_missing_clause.sc"))
        .output()
        .expect("reject an incomplete handler");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("missing handler clause `put`"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn non_capturing_function_values_run_through_indirect_calls() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "function_value_indirect.sc"))
        .output()
        .expect("run indirect function-value fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn higher_order_effect_rows_infer_pure_and_unsafe_callables() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "function_value_effect_generic.sc"))
        .output()
        .expect("run higher-order effect-row fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn pure_function_values_fill_wider_effect_slots() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "function_value_effect_subtyping.sc"))
        .output()
        .expect("run effect-row subtyping fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn emit_ir_and_check_cover_the_frontend() {
    let emitted = salic()
        .args(["emit-ir"])
        .arg(fixture("pass", "exit_42.sc"))
        .output()
        .expect("emit LLVM IR");
    assert!(emitted.status.success(), "{}", output_text(&emitted));
    let ir = String::from_utf8_lossy(&emitted.stdout);
    assert!(ir.contains("define i32 @main()"), "unexpected IR:\n{ir}");

    let checked = salic()
        .arg("check")
        .arg(fixture("pass", "condition.sc"))
        .output()
        .expect("check source");
    assert!(checked.status.success(), "{}", output_text(&checked));

    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/basics.sc");
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
        .arg(fixture("pass", "curried_call.sc"))
        .output()
        .expect("run curried program");
    assert_eq!(curried.status.code(), Some(42), "{}", output_text(&curried));

    let unit = salic()
        .arg("run")
        .arg(fixture("pass", "unit_main.sc"))
        .output()
        .expect("run unit program");
    assert!(unit.status.success(), "{}", output_text(&unit));

    let unit_values = salic()
        .arg("run")
        .arg(fixture("pass", "unit_values.sc"))
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
        .arg(fixture("pass", "short_circuit_return.sc"))
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
fn raw_pointer_read_and_write_run_with_expected_result() {
    for name in [
        "raw_pointer_read.sc",
        "raw_pointer_write.sc",
        "do_forwards_unsafe_color.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run raw pointer fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn raw_allocator_abi_allocates_aligned_storage_and_deallocates_it() {
    for name in [
        "raw_allocator_i32.sc",
        "raw_allocator_inferred.sc",
        "raw_allocator_layout.sc",
        "raw_pointer_offset.sc",
        "raw_pointer_offset_shared.sc",
        "raw_pointer_offset_unit.sc",
        "raw_pointer_borrow.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run raw allocator fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn raw_pointer_intrinsic_errors_report_their_cause() {
    for (name, expected) in [
        ("raw_offset_safe.sc", "requires an `unsafe` block"),
        (
            "raw_offset_non_pointer.sc",
            "requires `Ptr(T)` or `MutPtr(T)`",
        ),
        ("raw_trap_safe.sc", "requires an `unsafe` block"),
        (
            "raw_trap_arguments.sc",
            "expects one empty runtime argument group",
        ),
        ("raw_borrow_safe.sc", "requires an `unsafe` block"),
        (
            "raw_borrow_mut_immutable_pointer.sc",
            "requires a `MutPtr(T)`",
        ),
        ("raw_borrow_anchor_conflict.sc", "borrowed"),
        (
            "raw_borrow_mut_shared_anchor.sc",
            "requires a mutable borrow anchor",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid raw pointer offset fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn target_layout_intrinsics_cover_globals_aggregates_and_generic_instances() {
    for name in ["layout_intrinsics.sc", "layout_intrinsics_generic.sc"] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run target layout fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn alloc_box_owns_copy_and_resource_payloads() {
    for name in [
        "box_i32.sc",
        "box_resource.sc",
        "box_drop_once.sc",
        "box_nested_and_unit.sc",
        "box_recursive_layout.sc",
        "box_read.sc",
        "box_into_inner_drop_once.sc",
        "box_replace_drop.sc",
        "box_borrow.sc",
        "forget_resource.sc",
        "forget_temporary_resource.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run Box fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let trapped = salic()
        .arg("run")
        .arg(fixture("pass", "box_resource_drop_trap.sc"))
        .output()
        .expect("run Box recursive drop fixture");
    assert!(
        !trapped.status.success(),
        "boxed resource destructor did not run: {}",
        output_text(&trapped)
    );

    for name in [
        "box_borrow_then_replace.sc",
        "box_mut_borrow_conflict.sc",
        "box_borrow_then_into_inner.sc",
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check Box pointee borrow conflict");
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("borrowed"),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn alloc_vec_owns_copy_and_resource_elements() {
    for name in [
        "vec_copy.sc",
        "vec_unit.sc",
        "vec_resource.sc",
        "vec_borrow.sc",
        "vec_ordered_copy.sc",
        "vec_ordered_resource.sc",
        "vec_reorder_resource.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run Vec fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    for name in [
        "vec_read_out_of_bounds.sc",
        "vec_write_out_of_bounds.sc",
        "vec_replace_out_of_bounds.sc",
        "vec_swap_remove_out_of_bounds.sc",
        "vec_insert_out_of_bounds.sc",
        "vec_remove_out_of_bounds.sc",
        "vec_at_out_of_bounds.sc",
        "vec_at_access_mut_out_of_bounds.sc",
        "vec_swap_left_out_of_bounds.sc",
        "vec_swap_right_out_of_bounds.sc",
        "vec_capacity_overflow.sc",
        "vec_reserve_overflow.sc",
        "vec_zst_resource_drop_trap.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run out-of-bounds Vec fixture");
        assert!(
            !output.status.success(),
            "{name} did not trap: {}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "vec_resource_use_after_push.sc"))
        .output()
        .expect("check use after resource Vec push");
    assert!(
        !output.status.success(),
        "resource push unexpectedly copied"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("moved"),
        "{}",
        output_text(&output)
    );

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "vec_append_self_borrow.sc"))
        .output()
        .expect("check self append borrow conflict");
    assert!(
        !output.status.success(),
        "self append unexpectedly compiled"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already borrowed"),
        "{}",
        output_text(&output)
    );

    for name in ["vec_borrow_then_push.sc", "vec_mut_borrow_conflict.sc"] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check Vec element borrow conflict");
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("borrowed"),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn generic_inherent_extensions_infer_and_dispatch_concrete_instances() {
    for name in [
        "generic_inherent_extend.sc",
        "generic_inherent_reordered.sc",
        "generic_inherent_resource.sc",
        "generic_inherent_existing_instance.sc",
        "generic_enum_inherent_extend.sc",
        "generic_inherent_internal_dispatch.sc",
        "generic_inherent_from_generic_function.sc",
        "generic_extend_generic_member.sc",
        "box_methods.sc",
        "box_method_context_inference.sc",
        "access_generic.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run generic inherent extension fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "generic_inherent_member_shadow.sc"))
        .output()
        .expect("reject a member compile parameter that shadows its extension parameter");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("redeclares outer compile-time parameter"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn passing_keyword_generics_select_auto_copy_and_move() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "passing_generic.sc"))
        .output()
        .expect("run passing-generic fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "passing_copy_resource.sc"))
        .output()
        .expect("reject copy passing for a resource");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not implement Copy"),
        "{}",
        output_text(&output)
    );

    for (name, expected) in [
        ("passing_move_copy_use_after.sc", "moved"),
        ("passing_invalid_argument.sc", "invalid passing argument"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid passing-generic fixture");
        assert!(!output.status.success(), "{name}: {}", output_text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn where_copy_bounds_validate_generic_bodies_and_concrete_calls() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "where_copy_bound.sc"))
        .output()
        .expect("run generic function with a Copy bound");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (name, expected) in [
        ("where_copy_unsatisfied.sc", "not satisfied"),
        ("box_read_resource.sc", "not satisfied"),
        ("box_write_resource.sc", "not satisfied"),
        ("where_unknown_trait.sc", "unknown trait"),
        ("where_duplicate_predicate.sc", "duplicate where predicate"),
        ("where_trait_arity.sc", "argument count mismatch"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid where predicate");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn where_trait_bounds_enable_abstract_method_dispatch() {
    for name in ["where_method_dispatch.sc", "where_generic_trait_method.sc"] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run generic where-bound method dispatch");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "where_method_missing_bound.sc"))
        .output()
        .expect("reject unbounded abstract method dispatch");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown method"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn where_associated_equalities_enable_operator_dispatch() {
    for name in ["where_operator_output.sc", "where_associated_method.sc"] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run generic dispatch through an associated type equality");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "where_associated_type_mismatch.sc"))
        .output()
        .expect("reject an unsatisfied associated type equality");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("not satisfied"),
        "{}",
        output_text(&output)
    );

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "where_unknown_associated_type.sc"))
        .output()
        .expect("reject an unknown associated type equality");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown associated type"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn constrained_generic_extensions_select_members_per_instance() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "constrained_generic_extend.sc"))
        .output()
        .expect("run constrained generic extension");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (name, expected) in [
        ("constrained_extend_method_unsatisfied.sc", "unknown method"),
        ("box_read_method_resource.sc", "unknown method"),
        ("box_write_method_resource.sc", "unknown method"),
        (
            "constrained_extend_function_unsatisfied.sc",
            "not satisfied",
        ),
        ("constrained_extend_unknown_trait.sc", "unknown trait"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("reject an unsatisfied constrained extension member");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn generic_inherent_extensions_resolve_across_file_modules() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        r#"[package]
name = "generic-extend-modules"
version = "0.1.0"
edition = "2026"
"#,
    );
    project.write(
        "src/main.sc",
        "let main(): i32 = {\n  let cell = api.Cell.new(42)\n  cell.take()\n}\n",
    );
    project.write(
        "src/api.sc",
        "pub(package) let Cell(T: type) = struct(value: T)\n\
         extend(T: type) Cell(T) {\n\
           let new(move value: T): Cell(T) = { Cell(value) }\n\
           let take(move self)(): T = { self.value }\n\
         }\n",
    );

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .output()
        .expect("run package with a generic extension module");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn inherent_extensions_cannot_be_added_outside_the_defining_package() {
    let project = TestDirectory::new();
    project.write(
        "dep/salicin.toml",
        r#"[package]
name = "generic-cell"
version = "0.1.0"
edition = "2026"

[lib]
path = "src/lib.sc"
"#,
    );
    project.write(
        "dep/src/lib.sc",
        "pub let Cell(T: type) = struct(pub value: T)\n",
    );
    project.write(
        "app/salicin.toml",
        r#"[package]
name = "foreign-extend"
version = "0.1.0"
edition = "2026"

[dependencies]
dep = { path = "../dep" }
"#,
    );
    project.write(
        "app/src/main.sc",
        "extend(T: type) dep.Cell(T) {\n\
           let take(move self)(): T = { self.value }\n\
         }\n\
         let main(): i32 = { 0 }\n",
    );

    let output = salic()
        .arg("check")
        .arg(project.join("app"))
        .output()
        .expect("reject foreign inherent extension");
    assert!(
        !output.status.success(),
        "foreign extension unexpectedly passed"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("package that defines the type"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn raw_allocator_runtime_rejects_an_invalid_layout() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "raw_allocator_invalid_alignment.sc"))
        .output()
        .expect("run invalid allocator layout fixture");
    assert!(
        !output.status.success(),
        "invalid allocator layout unexpectedly succeeded: {}",
        output_text(&output)
    );
}

#[test]
fn raw_allocator_abi_can_be_replaced_by_strong_link_symbols() {
    let directory = TestDirectory::new();
    let source = directory.write(
        "main.sc",
        "let main(): i32 = {\n  let pointer = unsafe { raw_alloc(i32)(4, 4) }\n  unsafe { *pointer = 42 }\n  unsafe { raw_dealloc(pointer, 4, 4) }\n  0\n}\n",
    );
    let ir = directory.join("main.ll");
    let executable = directory.join("main");
    let custom = directory.write(
        "custom.c",
        "#include <stdint.h>\n#include <stdlib.h>\n_Alignas(64) static unsigned char storage[64];\nvoid *salicin_alloc(uint64_t size, uint64_t align) { (void)size; (void)align; return storage; }\nvoid salicin_dealloc(void *pointer, uint64_t size, uint64_t align) { (void)pointer; (void)size; (void)align; _Exit(42); }\n",
    );
    let emitted = salic()
        .args(["emit-ir"])
        .arg(&source)
        .arg("-o")
        .arg(&ir)
        .output()
        .expect("emit allocator ABI IR");
    assert!(emitted.status.success(), "{}", output_text(&emitted));

    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/allocator.c");
    let linked = Command::new("/usr/bin/clang")
        .args(["-Wno-override-module", "-x", "ir"])
        .arg(&ir)
        .args(["-x", "c", "-std=c11"])
        .arg(&custom)
        .arg(&runtime)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("link replacement allocator");
    assert!(linked.status.success(), "{}", output_text(&linked));

    let status = Command::new(&executable)
        .status()
        .expect("run replacement allocator fixture");
    assert_eq!(status.code(), Some(42));
}

#[test]
fn vec_drop_releases_its_allocation_through_the_allocator_abi() {
    let directory = TestDirectory::new();
    let source = directory.write(
        "main.sc",
        "use alloc.vec.Vec\n\nlet main(): i32 = {\n  let values: Vec(i32) = Vec(i32).new()\n  values.len()\n  0\n}\n",
    );
    let ir = directory.join("main.ll");
    let executable = directory.join("main");
    let custom = directory.write(
        "custom.c",
        "#include <stdint.h>\n#include <stdlib.h>\n_Alignas(64) static unsigned char storage[64];\nvoid *salicin_alloc(uint64_t size, uint64_t align) { (void)size; (void)align; return storage; }\nvoid salicin_dealloc(void *pointer, uint64_t size, uint64_t align) { (void)pointer; (void)size; (void)align; _Exit(42); }\n",
    );
    let emitted = salic()
        .args(["emit-ir"])
        .arg(&source)
        .arg("-o")
        .arg(&ir)
        .output()
        .expect("emit Vec allocator ABI IR");
    assert!(emitted.status.success(), "{}", output_text(&emitted));

    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/allocator.c");
    let linked = Command::new("/usr/bin/clang")
        .args(["-Wno-override-module", "-x", "ir"])
        .arg(&ir)
        .args(["-x", "c", "-std=c11"])
        .arg(&custom)
        .arg(&runtime)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("link Vec replacement allocator");
    assert!(linked.status.success(), "{}", output_text(&linked));

    let status = Command::new(&executable)
        .status()
        .expect("run Vec replacement allocator fixture");
    assert_eq!(status.code(), Some(42));
}

#[test]
fn m1_struct_programs_run_with_expected_result() {
    for name in [
        "struct_fields.sc",
        "struct_mutation.sc",
        "positional_constructor.sc",
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
fn type_constructor_aliases_run_and_report_kind_errors() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "type_constructor_alias.sc"))
        .output()
        .expect("run type-constructor alias fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (name, expected) in [
        ("type_alias_cycle.sc", "cyclic type alias"),
        ("type_alias_arity.sc", "argument count mismatch"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid type alias fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn type_constructor_aliases_cross_module_boundaries() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        "[package]\nname = \"alias-modules\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    project.write(
        "src/types.sc",
        "pub(package) let Cell(T: type) = struct(pub(package) value: T)\n\
         pub(package) let Family(T: type): type = Cell(T)\n\
         pub(package) let Constructor: (T: type): type = Cell\n\
         pub(package) let Scalar = i32\n",
    );
    project.write(
        "src/main.sc",
        "use types.{Family, Constructor, Scalar}\n\n\
         let main(): Scalar = {\n\
           let left: Family(i32) = Family(i32)(40)\n\
           let right = Constructor(2)\n\
           left.value + right.value\n\
         }\n",
    );

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .output()
        .expect("run project with imported type-constructor aliases");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn algebraic_effect_operations_check_their_instantiated_row() {
    let valid = salic()
        .arg("check")
        .arg(fixture("pass", "algebraic_effect_operations.sc"))
        .output()
        .expect("check algebraic effect operations");
    assert!(valid.status.success(), "{}", output_text(&valid));

    let invalid = salic()
        .arg("check")
        .arg(fixture("fail", "algebraic_effect_unhandled.sc"))
        .output()
        .expect("reject operation outside its effect row");
    assert!(!invalid.status.success());
    let stderr = String::from_utf8_lossy(&invalid.stderr);
    assert!(
        stderr.contains("requires custom effect") && stderr.contains("State(i32)"),
        "{}",
        output_text(&invalid)
    );
}

#[test]
fn m1_struct_errors_report_their_cause() {
    for (name, expected) in [
        ("unknown_field.sc", "unknown field"),
        ("constructor_missing_field.sc", "missing field"),
        ("constructor_duplicate_field.sc", "duplicate field"),
        ("constructor_mixed_arguments.sc", "mixed"),
        ("immutable_field_assignment.sc", "immutable"),
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
        "enum_match.sc",
        "nested_match.sc",
        "match_guard.sc",
        "match_literal_payload.sc",
        "match_literal_resource_guard.sc",
        "match_scalar.sc",
        "match_scalar_single_evaluation.sc",
        "if_let.sc",
        "partial_application.sc",
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
        ("non_exhaustive_match.sc", "exhaustive"),
        ("pattern_type_mismatch.sc", "pattern"),
        (
            "pattern_literal_payload_mismatch.sc",
            "pattern type mismatch",
        ),
        ("pattern_literal_payload_range.sc", "range"),
        ("match_scalar_constructor.sc", "cannot match scalar"),
        ("match_scalar_non_exhaustive.sc", "not exhaustive"),
        ("match_scalar_literal_range.sc", "range"),
        ("if_let_binding_scope.sc", "unknown"),
        ("temporary_borrow_partial.sc", "partial application"),
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
        "shared_borrow_call.sc",
        "mut_borrow_field_update.sc",
        "explicit_move_i32_once.sc",
        "borrow_released_after_complete_call.sc",
        "borrowed_unit_is_abi_erased.sc",
        "branch_move_does_not_pollute_sibling.sc",
        "disjoint_mut_field_borrows.sc",
        "inferred_copy_i32.sc",
        "move_then_return_preserves_other_branch.sc",
        "temporary_borrow_argument_order.sc",
        "temporary_mut_borrow_argument.sc",
        "temporary_borrow_argument_drop.sc",
        "temporary_borrow_method_argument.sc",
        "temporary_borrow_partial_call.sc",
        "explicit_borrow_types.sc",
        "region_scoped_borrow.sc",
        "returned_borrow.sc",
        "borrow_value_parameter.sc",
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
        ("use_after_move.sc", &["moved"][..]),
        ("use_after_explicit_move_i32.sc", &["moved"][..]),
        (
            "copy_non_copy.sc",
            &["requires `Copy`", "does not implement Copy"][..],
        ),
        (
            "double_mut_borrow.sc",
            &["mutable borrow", "already borrowed"][..],
        ),
        ("borrow_move_conflict.sc", &["move", "borrowed"][..]),
        (
            "same_field_mut_borrow_conflict.sc",
            &["mutable borrow", "already borrowed"][..],
        ),
        ("use_after_inferred_move.sc", &["moved"][..]),
        ("possibly_moved_after_branch.sc", &["possibly moved"][..]),
        ("both_branches_move.sc", &["moved"][..]),
        ("short_circuit_possibly_moves.sc", &["possibly moved"][..]),
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
fn explicit_borrow_type_errors_report_their_cause() {
    for (name, expected) in [
        ("borrow_type_kind_mismatch.sc", "borrow kind mismatch"),
        (
            "borrow_type_non_borrow_initializer.sc",
            "borrow value of local",
        ),
        ("borrow_type_pointee_mismatch.sc", "borrow pointee"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid explicit borrow type fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn region_frontend_errors_report_their_cause() {
    for (name, expected) in [
        ("region_undeclared_parameter.sc", "undeclared region `'a'"),
        ("region_undeclared_type.sc", "undeclared region `'a'"),
        ("region_duplicate.sc", "duplicate region parameter `'a'"),
        ("region_static_redeclared.sc", "predefined"),
        ("region_name_with_type_kind.sc", "must use the `region`"),
        ("region_plain_name.sc", "must start with `'`"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid region fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn returned_borrow_errors_report_their_cause() {
    for (name, expected) in [
        ("returned_borrow_local.sc", "borrow of a local value"),
        (
            "returned_borrow_temporary.sc",
            "cannot originate from a temporary",
        ),
        (
            "returned_borrow_shared_as_mut.sc",
            "shared borrow as a mutable borrow",
        ),
        ("returned_borrow_conflicting_write.sc", "already borrowed"),
        (
            "returned_borrow_missing_region.sc",
            "cannot infer the returned borrow region",
        ),
        ("returned_borrow_field.sc", "borrow-typed field"),
        (
            "returned_borrow_method.sc",
            "cannot originate from a temporary",
        ),
        ("returned_borrow_method_conflict.sc", "already borrowed"),
        ("returned_borrow_method_local.sc", "borrowing a local value"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid returned borrow fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn borrow_value_parameter_errors_report_their_cause() {
    for (name, expected) in [
        ("borrow_value_mut_moved.sc", "moved"),
        ("borrow_value_explicit_move.sc", "moved"),
        ("borrow_value_copy_mut.sc", "requires `Copy`"),
        ("borrow_value_block_escape_conflict.sc", "already borrowed"),
        ("borrow_value_partial.sc", "partial application"),
        (
            "borrow_value_local_escape.sc",
            "source is local or cannot be proven",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid borrow value parameter fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn v09_reinitialization_programs_run_with_expected_result() {
    for name in [
        "reinit_after_root_move.sc",
        "reinit_partial_field.sc",
        "reinit_root_move_field_by_field.sc",
        "reinit_after_both_if_branches.sc",
        "reinit_loop_backedge.sc",
        "reinit_after_explicit_copy_move.sc",
        "match_guard_copy_binding.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run v0.9 reinitialization fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn v09_reinitialization_errors_preserve_flow_safety() {
    for (name, expected) in [
        (
            "reinit_only_one_if_branch.sc",
            &["possibly", "uninitialized"][..],
        ),
        (
            "move_only_one_if_branch.sc",
            &["possibly", "uninitialized"][..],
        ),
        (
            "reinit_root_move_incomplete_fields.sc",
            &["uninitialized"][..],
        ),
        ("reinit_self_assignment_after_move.sc", &["moved"][..]),
        (
            "match_guard_move_non_copy_binding.sc",
            &["guard", "move"][..],
        ),
        (
            "reinit_widening_many_independent_branches.sc",
            &["possibly", "uninitialized"][..],
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid v0.9 reinitialization fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        for fragment in expected {
            assert!(
                stderr.contains(fragment),
                "{name} did not report `{fragment}`:\n{}",
                output_text(&output)
            );
        }
    }
}

#[test]
fn source_backed_copy_programs_run_with_expected_result() {
    for name in [
        "copy_nominal_repeated_and_parameters.sc",
        "copy_nominal_capture.sc",
        "copy_nominal_enum_array.sc",
        "copy_generic_blanket.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run source-backed Copy fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn source_backed_drop_glue_links_and_runs() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_glue.sc"))
        .output()
        .expect("run source-backed Drop program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn drop_runs_on_structured_scope_exits_without_double_drop() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_scope.sc"))
        .output()
        .expect("run structured Drop program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let trapped = salic()
        .arg("run")
        .arg(fixture("pass", "drop_trap.sc"))
        .output()
        .expect("run observable Drop trap");
    assert!(
        !trapped.status.success(),
        "Drop was not executed:\n{}",
        output_text(&trapped)
    );

    let generic_trapped = salic()
        .arg("run")
        .arg(fixture("pass", "drop_generic_blanket_trap.sc"))
        .output()
        .expect("run blanket generic Drop trap");
    assert!(
        !generic_trapped.status.success(),
        "blanket generic Drop was not executed:\n{}",
        output_text(&generic_trapped)
    );

    let partial_exit = salic()
        .arg("run")
        .arg(fixture("pass", "drop_partial_exit.sc"))
        .output()
        .expect("run partial-construction cleanup trap");
    assert!(
        !partial_exit.status.success(),
        "an owned constructor field leaked across return:\n{}",
        output_text(&partial_exit)
    );
}

#[test]
fn projection_drop_flags_preserve_unmoved_fields_and_rebuild_roots() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_partial_field.sc"))
        .output()
        .expect("run projection drop-flag program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let trapped = salic()
        .arg("run")
        .arg(fixture("pass", "drop_partial_field_trap.sc"))
        .output()
        .expect("run unmoved-field cleanup trap");
    assert!(
        !trapped.status.success(),
        "the unmoved sibling field was not dropped:\n{}",
        output_text(&trapped)
    );
}

#[test]
fn match_payload_moves_transfer_drop_ownership() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_payload.sc"))
        .output()
        .expect("run match payload drop program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let trapped = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_payload_trap.sc"))
        .output()
        .expect("run unmatched payload sibling cleanup trap");
    assert!(
        !trapped.status.success(),
        "the unmatched payload sibling was not dropped:\n{}",
        output_text(&trapped)
    );

    let nested = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_nested.sc"))
        .output()
        .expect("run nested match payload drop program");
    assert_eq!(nested.status.code(), Some(42), "{}", output_text(&nested));

    let nested_trap = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_nested_trap.sc"))
        .output()
        .expect("run nested match sibling cleanup trap");
    assert!(
        !nested_trap.status.success(),
        "the nested unmatched sibling was not dropped:\n{}",
        output_text(&nested_trap)
    );
}

#[test]
fn guarded_match_payload_moves_commit_only_after_success() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_guarded.sc"))
        .output()
        .expect("run guarded match payload program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let trapped = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_guarded_trap.sc"))
        .output()
        .expect("run guarded match rollback sibling trap");
    assert!(
        !trapped.status.success(),
        "guard rollback lost the unmatched sibling:\n{}",
        output_text(&trapped)
    );
}

#[test]
fn fn_once_resource_captures_drop_exactly_once() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_closure_once.sc"))
        .output()
        .expect("run resource-owning FnOnce closure");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (fixture_name, failure) in [
        (
            "drop_closure_abandon_trap.sc",
            "an abandoned closure environment was not dropped",
        ),
        (
            "drop_closure_early_trap.sc",
            "a capture staged before an early argument return was not dropped",
        ),
    ] {
        let trapped = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run closure capture cleanup trap");
        assert!(
            !trapped.status.success(),
            "{failure}:\n{}",
            output_text(&trapped)
        );
    }
}

#[test]
fn resource_partial_applications_transfer_and_drop_captures() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_partial_application.sc"))
        .output()
        .expect("run resource-owning partial applications");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (fixture_name, failure) in [
        (
            "drop_partial_application_abandon_trap.sc",
            "an abandoned partial capture was not dropped",
        ),
        (
            "drop_partial_application_early_trap.sc",
            "a partial capture staged before early return was not dropped",
        ),
    ] {
        let trapped = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run partial capture cleanup trap");
        assert!(
            !trapped.status.success(),
            "{failure}:\n{}",
            output_text(&trapped)
        );
    }
}

#[test]
fn callable_aliases_move_named_partial_closure_and_resource_environments() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "callable_alias.sc"))
        .output()
        .expect("run callable alias program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn concrete_partial_environments_return_across_function_boundaries() {
    for fixture_name in [
        "callable_return.sc",
        "callable_resource_return.sc",
        "closure_resource_return.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run returned callable environment");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{fixture_name} failed:\n{}",
            output_text(&output)
        );
    }

    let abandoned = salic()
        .arg("run")
        .arg(fixture("pass", "callable_resource_return_abandon_trap.sc"))
        .output()
        .expect("run abandoned returned callable environment");
    assert!(
        !abandoned.status.success(),
        "returned resource environment was not dropped:\n{}",
        output_text(&abandoned)
    );
}

#[test]
fn mutable_borrow_overwrite_drops_the_replaced_value() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_mut_borrow_overwrite.sc"))
        .output()
        .expect("run mutable-borrow overwrite program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (fixture_name, failure) in [
        (
            "drop_mut_borrow_root_trap.sc",
            "root overwrite did not drop the old referent",
        ),
        (
            "drop_mut_borrow_field_trap.sc",
            "field overwrite did not drop the old referent field",
        ),
    ] {
        let trapped = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run mutable-borrow overwrite trap");
        assert!(
            !trapped.status.success(),
            "{failure}:\n{}",
            output_text(&trapped)
        );
    }
}

#[test]
fn source_backed_copy_errors_report_their_cause() {
    for (name, expected) in [
        (
            "copy_non_copy.sc",
            &["requires `Copy`", "does not implement Copy"][..],
        ),
        (
            "copy_nominal_invalid_struct_impl.sc",
            &["Container", "cannot implement `Copy`", "Payload"][..],
        ),
        (
            "copy_nominal_invalid_enum_impl.sc",
            &["Message", "cannot implement `Copy`", "Payload"][..],
        ),
        (
            "copy_nominal_transitive_invalid_impl.sc",
            &["Branch", "Tree", "cannot implement `Copy`"][..],
        ),
        ("copy_nominal_explicit_move_reuse.sc", &["moved"][..]),
        (
            "copy_nominal_concrete_generic_impl.sc",
            &[
                "function `read`",
                "requires `Copy`",
                "Cell(i64)",
                "does not implement Copy",
            ][..],
        ),
        (
            "copy_generic_blanket_unproven.sc",
            &["blanket `Copy`", "not structurally valid"][..],
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid source-backed Copy fixture");
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
            !stderr.contains("$mono$type$"),
            "{name} leaked an internal monomorphization name:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_local_closure_programs_run_with_expected_result() {
    for name in [
        "capturing_closure.sc",
        "closure_shared_repeat.sc",
        "closure_capture_parameter.sc",
        "closure_curried_capture.sc",
        "closure_mut_capture.sc",
        "closure_move_once.sc",
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
        ("closure_escape_return.sc", "escape"),
        ("closure_partial_application.sc", "partial application"),
        ("closure_fnmut_immutable.sc", "FnMut"),
        ("closure_capture_borrow_conflict.sc", "borrowed"),
        ("closure_fnonce_twice.sc", "consumed"),
        ("closure_move_capture_source_use.sc", "moved"),
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
        .arg(fixture("pass", "block_mutation.sc"))
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
        .filter(|path| path.extension().is_some_and(|extension| extension == "sc"))
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
        .filter(|path| path.extension().is_some_and(|extension| extension == "sc"))
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
        "while_mutation.sc",
        "while_let.sc",
        "continue.sc",
        "continue_cleanup.sc",
        "loop_break_value.sc",
        "fixed_array_index.sc",
        "array_index_assignment.sc",
        "array_constant_index_place.sc",
        "array_index_move_reinitialize.sc",
        "array_nested_constant_index_place.sc",
        "array_index_raw_pointer.sc",
        "array_non_copy_element.sc",
        "array_resource_drop.sc",
        "array_resource_nested_drop.sc",
        "array_resource_overwrite_drop.sc",
        "array_resource_temporary_index.sc",
        "dynamic_array_index.sc",
        "empty_array_typed.sc",
        "nested_loop_break.sc",
        "loop_move_then_break.sc",
        "for_iterator.sc",
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
fn named_arguments_select_function_overloads_in_resolved_sources() {
    for name in [
        "function_overload_named.sc",
        "generic_overload_named.sc",
        "inherent_overload_named.sc",
        "trait_overload_named.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run named function overload fixture");
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
        ("array_index_type.sc", "index"),
        ("array_length_mismatch.sc", "length"),
        ("array_constant_oob.sc", "out of bounds"),
        ("array_negative_oob.sc", "out of bounds"),
        ("array_empty_without_context.sc", "empty array"),
        ("array_resource_dynamic_index.sc", "requires Copy"),
        ("array_resource_element_use_after_move.sc", "moved"),
        ("array_resource_partial_root_move.sc", "moved"),
        ("array_dynamic_index_assignment.sc", "compile-time"),
        ("array_index_borrow_conflict.sc", "borrowed"),
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
        ("break_outside_loop.sc", "outside"),
        ("continue_outside_loop.sc", "outside"),
        ("while_break_value.sc", "while"),
        ("loop_break_type_mismatch.sc", "type mismatch"),
        ("loop_backedge_move.sc", "move"),
        ("while_let_binding_scope.sc", "unknown"),
        ("for_missing_into_iterator.sc", "IntoIterator"),
        ("for_missing_iterator.sc", "Iterator"),
        ("for_break_value.sc", "type mismatch"),
        ("for_refutable_pattern.sc", "irrefutable"),
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
        .arg(fixture("pass", "dynamic_array_oob.sc"))
        .output()
        .expect("run dynamically out-of-bounds array fixture");
    assert!(
        !output.status.success(),
        "out-of-bounds indexing unexpectedly succeeded:\n{}",
        output_text(&output)
    );
}

#[test]
fn invalid_builtin_division_and_remainder_trap() {
    for name in [
        "runtime_division_by_zero.sc",
        "runtime_remainder_overflow.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run invalid built-in arithmetic fixture");
        assert!(
            !output.status.success(),
            "{name} unexpectedly avoided its arithmetic trap:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_inherent_members_run_with_expected_result() {
    for name in [
        "inherent_reset_and_constant.sc",
        "inherent_grouped_shared_method.sc",
        "inherent_move_receiver.sc",
        "inherent_associated_function.sc",
        "inherent_associated_field_same_name.sc",
        "inherent_method_and_associated_same_name.sc",
        "inherent_local_shadows_type.sc",
        "inherent_recursive_method.sc",
        "inherent_enum_method.sc",
        "inherent_receiver_loan_released.sc",
        "inherent_temporary_borrow_receiver.sc",
        "inherent_temporary_mut_receiver.sc",
        "inherent_temporary_mut_resource_receiver.sc",
        "inherent_temporary_resource_receiver.sc",
        "inherent_disjoint_forward_extend.sc",
        "qualified_inherent_method.sc",
        "qualified_trait_generic_method.sc",
        "self_expression_members.sc",
        "self_expression_generic.sc",
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
        ("inherent_field_method_conflict.sc", "conflicts with field"),
        ("inherent_duplicate_method.sc", "duplicate inherent method"),
        (
            "inherent_duplicate_associated.sc",
            "duplicate associated member",
        ),
        (
            "inherent_variant_associated_conflict.sc",
            "conflicts with variant",
        ),
        ("inherent_mut_receiver_immutable.sc", "immutable"),
        ("inherent_unknown_target.sc", "unknown extension target"),
        ("inherent_trait_extension_pending.sc", "unknown trait"),
        ("inherent_bound_method_value.sc", "must be called"),
        ("inherent_associated_function_value.sc", "must be called"),
        ("inherent_temporary_mut_partial.sc", "partial application"),
        ("inherent_move_receiver_reuse.sc", "moved"),
        ("inherent_borrowed_partial.sc", "partial application"),
        ("inherent_receiver_borrow_conflict.sc", "borrowed"),
        ("inherent_non_nominal_target.sc", "nominal"),
        ("qualified_method_bad_label.sc", "unlabeled or named `self`"),
        (
            "qualified_method_missing_receiver.sc",
            "exactly one argument",
        ),
        ("qualified_method_wrong_receiver.sc", "requires receiver"),
        (
            "qualified_method_borrowed_partial.sc",
            "partial application",
        ),
        ("self_expression_outside_extend.sc", "only available inside"),
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
        "generic_identity.sc",
        "generic_multiple_instances.sc",
        "generic_type_application_partial.sc",
        "generic_composition.sc",
        "generic_same_instance_recursion.sc",
        "generic_call_inside_closure.sc",
        "generic_validation_rollback.sc",
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
        ("generic_unused_invalid_body.sc", "type mismatch"),
        ("generic_parameter_moved_twice.sc", "moved"),
        ("generic_missing_return_type.sc", "return type"),
        ("generic_unconstrained_member.sc", "generic parameter"),
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
        "generic_struct.sc",
        "generic_nested_struct.sc",
        "generic_enum_match.sc",
        "generic_function_constructs_nominal.sc",
        "generic_nominal_multiple_instances.sc",
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
        ("generic_nominal_unknown_field_type.sc", "unknown type"),
        ("generic_nominal_recursive_layout.sc", "infinite size"),
        ("generic_nominal_argument_count.sc", "argument count"),
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
        "infer_generic_function.sc",
        "infer_function_from_expected.sc",
        "infer_generic_struct.sc",
        "infer_nested_generic_struct.sc",
        "infer_nominal_from_expected.sc",
        "infer_generic_enum_variant.sc",
        "infer_runtime_partial.sc",
        "infer_argument_once.sc",
        "infer_constraint_order.sc",
        "infer_fresh_constructor.sc",
        "infer_named_arguments.sc",
        "infer_nonempty_block.sc",
        "infer_borrow_temporary.sc",
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
        ("infer_conflicting_arguments.sc", "conflicting"),
        ("infer_expected_conflict.sc", "conflicting"),
        ("infer_unconstrained.sc", "cannot infer"),
        ("infer_incomplete_application.sc", "requires explicit"),
        ("infer_nested_hole.sc", "not an expression"),
        ("infer_moved_argument.sc", "moved"),
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
        "trait_unique_method.sc",
        "trait_associated_output.sc",
        "trait_generic_nominal_impl.sc",
        "trait_generic_blanket_impl.sc",
        "trait_disjoint_blanket_impls.sc",
        "trait_default_method.sc",
        "trait_temporary_receiver.sc",
        "trait_temporary_mut_receiver.sc",
        "trait_inherent_precedence.sc",
        "trait_declaration_order.sc",
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
        ("trait_unknown_trait.sc", "unknown trait"),
        ("trait_duplicate_impl.sc", "duplicate trait implementation"),
        ("trait_missing_method.sc", "missing trait method"),
        ("trait_missing_type.sc", "missing associated type"),
        ("trait_extra_member.sc", "unknown trait member"),
        ("trait_pass_mode_mismatch.sc", "signature mismatch"),
        ("trait_group_mismatch.sc", "signature mismatch"),
        ("trait_return_mismatch.sc", "signature mismatch"),
        ("trait_ambiguous_method.sc", "ambiguous trait method"),
        (
            "trait_generic_impl_pending.sc",
            "generic trait implementation",
        ),
        (
            "trait_generic_uninstantiated_body.sc",
            "unknown name `missing`",
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
        "add_trait_nominal_pair.sc",
        "add_trait_nominal_i32_nominal_output.sc",
        "add_trait_nominal_i32_scalar_output.sc",
        "add_trait_builtin_integer_precedence.sc",
        "add_trait_operands_once.sc",
        "add_trait_expected_output.sc",
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
        ("add_trait_missing_impl.sc", "Add"),
        ("add_trait_rhs_mismatch.sc", "Add"),
        ("add_trait_ambiguous_literal.sc", "ambiguous"),
        ("add_trait_use_after_move.sc", "moved"),
        ("add_trait_rhs_use_after_move.sc", "moved"),
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
fn arithmetic_trait_programs_run_with_expected_result() {
    for name in [
        "arithmetic_traits_nominal_dispatch.sc",
        "arithmetic_trait_operands_once.sc",
        "arithmetic_trait_expected_output.sc",
        "compound_assign_builtin.sc",
        "compound_assign_trait.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run arithmetic-trait fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn arithmetic_trait_errors_report_their_cause() {
    for (name, expected) in [
        ("arithmetic_trait_ambiguous_literal.sc", "ambiguous"),
        ("arithmetic_trait_rhs_mismatch.sc", "Div"),
        ("arithmetic_trait_use_after_move.sc", "moved"),
        ("arithmetic_trait_scalar_rhs_use_after_move.sc", "moved"),
        ("compound_assign_immutable.sc", "immutable"),
        ("compound_assign_missing_impl.sc", "AddAssign"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid arithmetic-trait fixture");
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
        "prelude_option_some.sc",
        "prelude_option_none.sc",
        "prelude_result_ok.sc",
        "prelude_result_err.sc",
        "prelude_nested_option_result.sc",
        "prelude_multiple_instances.sc",
        "prelude_inferred_variants.sc",
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
        ("prelude_redefine_option.sc", "Option"),
        ("prelude_redefine_result.sc", "Result"),
        ("prelude_option_arity.sc", "argument count"),
        ("prelude_result_arity.sc", "argument count"),
        ("prelude_option_payload_mismatch.sc", "conflicting"),
        ("prelude_result_ok_payload_mismatch.sc", "conflicting"),
        ("prelude_result_err_payload_mismatch.sc", "conflicting"),
        ("prelude_option_expected_mismatch.sc", "conflicting"),
        ("prelude_result_expected_mismatch.sc", "conflicting"),
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
        "coalesce_option_some_short_circuit.sc",
        "coalesce_option_none_fallback.sc",
        "coalesce_result_ok_short_circuit.sc",
        "coalesce_result_err_fallback.sc",
        "coalesce_right_associative.sc",
        "coalesce_logical_or_precedence.sc",
        "coalesce_match_precedence_nested_option.sc",
        "coalesce_lhs_once.sc",
        "coalesce_nested_result_payload.sc",
        "coalesce_infer_option_none.sc",
        "coalesce_infer_result_err.sc",
        "coalesce_infer_right_associative_none.sc",
        "coalesce_infer_local_without_annotation.sc",
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
        ("coalesce_option_use_after_move.sc", "moved"),
        ("coalesce_result_use_after_move.sc", "moved"),
        ("coalesce_option_rhs_mismatch.sc", "type mismatch"),
        ("coalesce_result_rhs_mismatch.sc", "type mismatch"),
        ("coalesce_non_container_lhs.sc", "Option"),
        (
            "coalesce_infer_result_error_unconstrained.sc",
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
fn explicit_result_values_and_throws_handlers_run_with_expected_result() {
    for name in [
        "try_full_container_unchanged.sc",
        "do_try_boundary.sc",
        "do_function_boundary.sc",
        "do_forwards_throws.sc",
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
fn removed_postfix_try_reports_a_migration_diagnostic() {
    for (name, expected) in [
        ("try_non_container_operand.sc", "postfix `.try` was removed"),
        ("result_return_type_mismatch.sc", "type mismatch"),
        (
            "result_requires_explicit_constructor.sc",
            "integer literal cannot be used where",
        ),
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
fn eq_operator_protocol_runs_with_borrowed_operands() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "eq_operator_trait.sc"))
        .output()
        .expect("run Eq operator fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn partial_ord_protocol_preserves_unordered_comparisons() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "partial_ord_operator_trait.sc"))
        .output()
        .expect("run PartialOrd operator fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn unary_operator_protocols_run_with_associated_outputs() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "unary_operator_traits.sc"))
        .output()
        .expect("run unary operator fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn bitwise_protocols_run_and_invalid_shifts_trap() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "bitwise_operator_traits.sc"))
        .output()
        .expect("run bitwise operator fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for name in ["shift_out_of_range.sc", "shift_negative.sc"] {
        let invalid = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run invalid shift fixture");
        assert!(
            !invalid.status.success(),
            "invalid shift in {name} unexpectedly succeeded"
        );
    }
}

#[test]
fn m2_optional_chain_programs_run_with_expected_result() {
    for name in [
        "chain_option_some_field.sc",
        "chain_option_none_field.sc",
        "chain_result_ok_field.sc",
        "chain_result_err_field.sc",
        "chain_success_type_changes.sc",
        "chain_consecutive_fields.sc",
        "chain_option_method.sc",
        "chain_result_method.sc",
        "chain_borrowed_method.sc",
        "chain_option_method_arguments_are_lazy.sc",
        "chain_result_method_arguments_are_lazy.sc",
        "chain_inferred_inputs.sc",
        "chain_lhs_once.sc",
        "chain_method_result_is_nested.sc",
        "chain_then_coalesce.sc",
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
        ("chain_non_container.sc", "Option"),
        ("chain_unknown_field.sc", "missing"),
        ("chain_unknown_method.sc", "missing"),
        ("chain_mut_borrow_method.sc", "mutable-borrow"),
        ("chain_method_partial_application.sc", "fully applied"),
        ("chain_use_after_move.sc", "moved"),
        ("chain_nested_result_not_flattened.sc", "type mismatch"),
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
fn throws_programs_run_with_expected_result() {
    for name in [
        "throw_result_err_propagate.sc",
        "throw_error_once.sc",
        "throw_if_flow.sc",
        "throw_generic_error.sc",
        "throw_unit_error.sc",
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
fn throws_errors_report_their_cause() {
    for (name, expected) in [
        ("throw_in_option_return.sc", "with(throws(Error))"),
        ("throw_in_plain_return.sc", "with(throws(Error))"),
        ("throw_in_global.sc", "global"),
        ("throw_in_closure.sc", "with(throws(Error))"),
        ("throw_omitted_return_type.sc", "with(throws(Error))"),
        ("throw_error_type_mismatch.sc", "expected"),
        ("throw_without_value.sc", "expression"),
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
        r#"pub let Number = struct(value: i32)
let Secret = trait {
  let reveal(borrow self)(): i32
}
extend Number: Secret {
  let reveal(borrow self)(): i32 = { self.value }
}
pub let make(): Number = { Number(value: 21) }
pub let maybe(): Option(Number) = { Option(Number).Some(make()) }
pub let reveal(T: type)(move number: Number): i32 = { number.reveal() }
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
        r#"use core.ops.Add

pub let Number = struct(value: i32)
extend Number: Add(Number) {
  let Output = Number
  let add(move self)(move rhs: Number): Number = { Number(self.value + rhs.value) }
}
pub let make(value: i32): Number = { Number(value) }
pub let value(move number: Number): i32 = { number.value }
pub let maybe(value: i32): Option(i32) = { Option(i32).Some(value) }
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
        r#"pub let Option(T: type) = enum { Some(T), None }
pub let make_option(): Option(i32) = { Option(i32).Some(42) }

pub let Add(Rhs: type) = trait {
  let Output: type
  let add(move self)(move rhs: Rhs): Output
}
pub let Sub(Rhs: type) = trait {
  let Output: type
  let sub(move self)(move rhs: Rhs): Output
}
pub let Number = struct(value: i32)
extend Number: Add(Number) {
  let Output = Number
  let add(move self)(move rhs: Number): Number = { Number(self.value + rhs.value) }
}
extend Number: Sub(Number) {
  let Output = Number
  let sub(move self)(move rhs: Number): Number = { Number(self.value - rhs.value) }
}
pub let make_number(value: i32): Number = { Number(value) }
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
        .expect("reject a module type spoofing core Option");
    assert_eq!(
        fake_option.status.code(),
        Some(1),
        "{}",
        output_text(&fake_option)
    );
    assert!(
        String::from_utf8_lossy(&fake_option.stderr)
            .contains("requires `Option(T)` or `Result(T, E)`"),
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
        .expect("reject a module trait spoofing core Add");
    assert_eq!(
        fake_add.status.code(),
        Some(1),
        "{}",
        output_text(&fake_add)
    );
    assert!(
        String::from_utf8_lossy(&fake_add.stderr).contains("no matching `Add` implementation"),
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
        .expect("reject a module trait spoofing core Sub");
    assert_eq!(
        fake_sub.status.code(),
        Some(1),
        "{}",
        output_text(&fake_sub)
    );
    assert!(
        String::from_utf8_lossy(&fake_sub.stderr).contains("no matching `Sub` implementation"),
        "{}",
        output_text(&fake_sub)
    );

    workspace.write(
        "app/src/main.sc",
        "use root.fake as Option\nlet main(): i32 = { Option() }\n",
    );
    let module_option = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a module alias falling back to core Option");
    assert_eq!(
        module_option.status.code(),
        Some(1),
        "{}",
        output_text(&module_option)
    );
    assert!(
        String::from_utf8_lossy(&module_option.stderr)
            .contains("module `Option` cannot be used as a value or callable"),
        "{}",
        output_text(&module_option)
    );

    workspace.write(
        "app/src/main.sc",
        r#"use root.fake as Add
let Number = struct(value: i32)
extend Number: Add(Number) {
  let Output = i32
  let add(move self)(move rhs: Number): i32 = { self.value + rhs.value }
}
let main(): i32 = { Number(20) + Number(22) }
"#,
    );
    let module_add = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a module alias falling back to core Add");
    assert_eq!(
        module_add.status.code(),
        Some(1),
        "{}",
        output_text(&module_add)
    );
    assert!(
        String::from_utf8_lossy(&module_add.stderr)
            .contains("module `Add` cannot be used as a type"),
        "{}",
        output_text(&module_add)
    );

    workspace.write("app/src/never.sc", "let marker = 0\n");
    workspace.write(
        "app/src/main.sc",
        "let stop(): never = { loop {} }\nlet main(): i32 = { 42 }\n",
    );
    let module_never = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a child module falling back to core never");
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
        r#"pub let Token = struct(value: i32)
pub let make(value: i32): Token = { Token(value) }
"#,
    );
    workspace.write(
        "app/src/main.sc",
        r#"extend dep.Token: Copy {}
let main(): i32 = { 42 }
"#,
    );

    let app = workspace.join("app");
    let orphan = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject a downstream Copy implementation for an upstream type");
    assert_eq!(orphan.status.code(), Some(1), "{}", output_text(&orphan));
    let stderr = String::from_utf8_lossy(&orphan.stderr);
    assert!(
        stderr.contains("`Copy` for") && stderr.contains("package that defines the type"),
        "{}",
        output_text(&orphan)
    );

    workspace.write(
        "dep/src/lib.sc",
        r#"pub let Token = struct(value: i32)
extend Token: Copy {}
pub let make(value: i32): Token = { Token(value) }
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
        .expect("use an upstream Copy implementation in a downstream package");
    assert_eq!(
        owner_impl.status.code(),
        Some(42),
        "{}",
        output_text(&owner_impl)
    );

    workspace.write("app/src/fake.sc", "pub let Copy = trait {}\n");
    workspace.write(
        "app/src/main.sc",
        r#"use root.fake.Copy as FakeCopy
let Local = struct(value: i32)
extend Local: FakeCopy {}
let read(copy local: Local): i32 = { local.value }
let main(): i32 = { read(Local(42)) }
"#,
    );

    let alias_spoof = salic()
        .arg("check")
        .arg(&app)
        .output()
        .expect("reject an alias of a fake Copy trait as a language marker");
    assert_eq!(
        alias_spoof.status.code(),
        Some(1),
        "{}",
        output_text(&alias_spoof)
    );
    let stderr = String::from_utf8_lossy(&alias_spoof.stderr);
    assert!(
        stderr.contains("requires `Copy`") && stderr.contains("does not implement Copy"),
        "{}",
        output_text(&alias_spoof)
    );

    workspace.write(
        "app/src/fake.sc",
        r#"pub let Copy = trait {}
pub let Token = struct(value: i32)

extend Token: Copy {}

pub let make(value: i32): Token = { Token(value) }
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
        .expect("reject a module trait spoofing core Copy semantics");
    assert_eq!(spoof.status.code(), Some(1), "{}", output_text(&spoof));
    let stderr = String::from_utf8_lossy(&spoof.stderr);
    assert!(
        stderr.contains("requires `Copy`") && stderr.contains("does not implement Copy"),
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
        "pub let Token = struct(pub value: i32)\npub let make(value: i32): Token = { Token(value: value) }\n",
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
        r#"let stop(): never = { loop {} }
let absurd(move value: never): i32 = { value }
let propagate(move value: never): Result(i32, ()) = { value }
let throw_never(move value: never): i32 with(throws(())) = { throw value }
let Empty = enum {}
let Holder = struct(value: Empty)
let project(move holder: Holder): i32 = { holder.value }
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
        "src/math.sc",
        r#"pub(package) let Number = struct(value: i32)
let Read = trait {
  let read(borrow self)(): i32
}
extend Number: Read {
  let read(borrow self)(): i32 = { self.value }
}
pub(package) let answer(): i32 = {
  let number = Number(value: 40)
  number.read()
}
"#,
    );
    project.write(
        "src/net/http.sc",
        r#"pub(package) let Reply = struct(pub(package) value: i32)
pub(package) let Status = enum {
  Ok(i32),
  Err,
}
pub(package) let reply(): Reply = { Reply(value: 0) }
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
        r#"pub(package) let Record = struct(secret: i32, pub(package) open: i32)
pub(package) let Event = enum { Named(secret: i32), Empty }
pub(package) let record(): Record = { Record(secret: 20, open: 22) }
pub(package) let event(): Event = { Event.Named(secret: 42) }
"#,
    );
    private_project.write(
        "src/main.sc",
        r#"let read(): i32 = { data.record().secret }
let build(): data.Record = { data.Record(secret: 20, open: 22) }
let unpack(): i32 = { data.event() match {
  data.Event.Named(secret: value) => value,
  data.Event.Empty => 0,
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
        r#"pub let Record = struct(pub value: i32)
pub let Event = enum { Named(pub value: i32), Empty }
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
  let record = dep.Record(value: 20)
  let event = dep.Event.Named(value: 22)
  let extra = event match {
    dep.Event.Named(value: value) => value,
    dep.Event.Empty => 0,
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
        r#"pub(package) let Number = struct(pub(package) value: i32)
pub(package) let Outcome = enum {
  Ready(i32),
  Empty,
}
pub(package) let zero(): i32 = { 0 }
pub(package) let increment(value: i32): i32 = { value + 1 }
pub(package) let make_number(value: i32): Number = { Number(value: value) }
"#,
    );
    project.write("src/nested.sc", "let parent_bonus(): i32 = { 2 }\n");
    project.write(
        "src/nested/deep.sc",
        r#"use root.kit.{Number, Outcome, increment}
use root.kit.make_number as make
use root.kit as utilities
use self.local_bonus as local
use super.parent_bonus as parent
use root.root_bonus as from_root

let local_bonus(): i32 = { 1 }

pub(package) let answer(): i32 = {
  let number: Number = make(35)
  let outcome: Outcome = Outcome.Ready(increment(number.value))
  let value = outcome match {
    Outcome.Ready(value) => value,
    Outcome.Empty => 0
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
use root.second.answer as selected
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
