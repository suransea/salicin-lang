use crate::support::*;

#[test]
fn named_arguments_select_function_overloads_in_resolved_sources() {
    let fixtures = [
        "function_overload_named.sc",
        "generic_overload_named.sc",
        "inherent_overload_named.sc",
        "trait_overload_named.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
        ("for_refutable_pattern.sc", "pattern type mismatch"),
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
    let fixtures = [
        "runtime_division_by_zero.sc",
        "runtime_remainder_overflow.sc",
    ];
    for (name, output) in trapping_fixture_outputs_in_parallel(&fixtures) {
        assert!(
            !output.status.success(),
            "{name} unexpectedly avoided its arithmetic trap:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_inherent_members_run_with_expected_result() {
    let fixtures = [
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
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
    let fixtures = [
        "generic_identity.sc",
        "generic_multiple_instances.sc",
        "generic_type_application_partial.sc",
        "generic_composition.sc",
        "generic_same_instance_recursion.sc",
        "generic_call_inside_closure.sc",
        "generic_validation_rollback.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
    let fixtures = [
        "generic_struct.sc",
        "generic_nested_struct.sc",
        "generic_enum_match.sc",
        "generic_function_constructs_nominal.sc",
        "generic_nominal_multiple_instances.sc",
        "generic_nominal_access.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
    let fixtures = [
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
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
    let fixtures = [
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
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
    let fixtures = [
        "add_trait_nominal_pair.sc",
        "add_trait_nominal_i32_nominal_output.sc",
        "add_trait_nominal_i32_scalar_output.sc",
        "add_trait_builtin_integer_precedence.sc",
        "add_trait_operands_once.sc",
        "add_trait_expected_output.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
    let fixtures = [
        "arithmetic_traits_nominal_dispatch.sc",
        "arithmetic_trait_operands_once.sc",
        "arithmetic_trait_expected_output.sc",
        "arithmetic_trait_scalar_rhs_auto_reuse.sc",
        "add_trait_copy_operands_reusable.sc",
        "compound_assign_builtin.sc",
        "compound_assign_trait.sc",
        "source_defined_primitive_ops.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
fn m2_core_option_and_result_programs_run_with_expected_result() {
    let fixtures = [
        "core_option_some.sc",
        "core_option_none.sc",
        "core_result_ok.sc",
        "core_result_err.sc",
        "core_nested_option_result.sc",
        "core_multiple_instances.sc",
        "core_inferred_variants.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_core_option_and_result_errors_report_their_cause() {
    for (name, expected) in [
        ("core_redefine_option.sc", "Option"),
        ("core_redefine_result.sc", "Result"),
        ("core_option_arity.sc", "argument count"),
        ("core_result_arity.sc", "argument count"),
        ("core_option_payload_mismatch.sc", "conflicting"),
        ("core_result_ok_payload_mismatch.sc", "conflicting"),
        ("core_result_err_payload_mismatch.sc", "conflicting"),
        ("core_option_expected_mismatch.sc", "conflicting"),
        ("core_result_expected_mismatch.sc", "conflicting"),
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
    let fixtures = [
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
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
    let fixtures = [
        "try_full_container_unchanged.sc",
        "do_try_boundary.sc",
        "do_function_boundary.sc",
        "do_forwards_throws.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn postfix_try_is_an_ordinary_member_name() {
    for (name, expected) in [
        (
            "try_non_container_operand.sc",
            "member access requires a struct value",
        ),
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

    let invalid_shifts = ["shift_out_of_range.sc", "shift_negative.sc"];
    for (name, invalid) in trapping_fixture_outputs_in_parallel(&invalid_shifts) {
        assert!(
            !invalid.status.success(),
            "invalid shift in {name} unexpectedly succeeded"
        );
    }
}

#[test]
fn compile_time_argument_diagnostics_name_binders_kinds_and_groups() {
    for (name, expected) in [
        (
            "infer_unconstrained.sc",
            vec!["argument `T`", "kind `type`", "for `make`"],
        ),
        (
            "infer_unconstrained_constructor.sc",
            vec![
                "argument `F`",
                "kind `(1 type parameter): type`",
                "for `make`",
            ],
        ),
        (
            "generic_nominal_argument_count.sc",
            vec![
                "argument count mismatch in group 1",
                "`T` of kind `type`",
                "found 2",
            ],
        ),
        (
            "type_constructor_unknown_label.sc",
            vec![
                "argument label `Element`",
                "expected one of `T` of kind `type`",
            ],
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check compile-time argument diagnostic fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        let stderr = String::from_utf8_lossy(&output.stderr);
        for expected in expected {
            assert!(
                stderr.contains(expected),
                "{name}: expected `{expected}`:\n{}",
                output_text(&output)
            );
        }
        assert!(
            !stderr.contains('$'),
            "{name} leaked an internal compile-time name:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m2_optional_chain_programs_run_with_expected_result() {
    let fixtures = [
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
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
    let fixtures = [
        "throw_result_err_propagate.sc",
        "throw_error_once.sc",
        "throw_if_flow.sc",
        "throw_generic_error.sc",
        "throw_unit_error.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
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
        ("throw_in_option_return.sc", "handle it with `try { ... }`"),
        ("throw_in_plain_return.sc", "handle it with `try { ... }`"),
        ("throw_in_global.sc", "global"),
        ("throw_in_closure.sc", "handle it with `try { ... }`"),
        (
            "throw_omitted_return_type.sc",
            "handle it with `try { ... }`",
        ),
        ("throw_error_type_mismatch.sc", "requires `Throws(i32)`"),
        (
            "throw_without_value.sc",
            "standard-library item `throw` is not in the prelude",
        ),
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
