use crate::support::*;

#[test]
fn effectful_for_preserves_iterator_state_and_cleanup() {
    let fixtures = ["for_failure.sc", "for_failure_cleanup.sc"];
    for (_fixture_name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
    }
}

#[test]
fn owned_nominal_state_crosses_handler_resume_and_abandonment() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "algebraic_effect_owned_state.sc"))
        .output()
        .expect("run owned handler state fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn owned_nominal_state_crosses_repeated_effectful_calls() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "algebraic_effect_owned_state_calls.sc"))
        .output()
        .expect("run owned handler call-state fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn disjoint_owned_projections_cross_repeated_effectful_calls() {
    for (name, output) in batched_native_fixture_outputs(&[
        "algebraic_effect_owned_field_calls.sc",
        "algebraic_effect_disjoint_field_calls.sc",
        "algebraic_effect_disjoint_index_calls.sc",
    ]) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn owned_roots_cross_concrete_residual_effect_rows() {
    for (name, output) in batched_native_fixture_outputs(&[
        "algebraic_effect_owned_residual_failure_outer.sc",
        "algebraic_effect_owned_residual_failure_inner.sc",
        "algebraic_effect_owned_residual_nominal.sc",
        "algebraic_effect_owned_residual_unsafe.sc",
    ]) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn owned_roots_cross_direct_and_mutual_recursive_effectful_calls() {
    for (name, output) in batched_native_fixture_outputs(&[
        "algebraic_effect_owned_recursive_call.sc",
        "algebraic_effect_owned_mutual_recursion.sc",
    ]) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn indexed_nominal_state_crosses_effectful_calls_once() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "algebraic_effect_owned_index_calls.sc"))
        .output()
        .expect("run indexed handler state fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn effectful_indexed_borrow_out_of_bounds_traps() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "algebraic_effect_owned_index_oob.sc"))
        .output()
        .expect("run out-of-bounds indexed handler state fixture");
    assert!(
        !output.status.success(),
        "out-of-bounds effectful indexed borrow unexpectedly succeeded:\n{}",
        output_text(&output)
    );
}

#[test]
fn owned_nominal_state_crosses_effectful_loop_backedges() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "algebraic_effect_owned_state_loop.sc"))
        .output()
        .expect("run owned handler loop-state fixture");
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
fn functional_protocols_forward_callback_effects() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "functional_effect_forwarding.sc"))
        .output()
        .expect("run functional effect-forwarding fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn capturing_callable_bridge_preserves_runtime_ownership_and_effects() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "capturing_callable_bridge.sc"))
        .output()
        .expect("run capturing callable bridge fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (name, expected) in [
        ("capturing_callable_bridge_overlap.sc", "borrowed"),
        ("capturing_callable_bridge_escape.sc", "escape"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check rejected capturing callable bridge");
        assert!(!output.status.success(), "{name}: {}", output_text(&output));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name}: expected `{expected}` diagnostic: {}",
            output_text(&output)
        );
        assert!(
            !stderr.contains("$callable$"),
            "{name}: generated bridge name leaked: {}",
            output_text(&output)
        );
    }
}

#[test]
fn generic_associated_constructors_lower_compile_time_kinds() {
    for (name, output) in
        batched_native_fixture_outputs(&["gat_borrow_family.sc", "gat_usize_family.sc"])
    {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "gat_constructor_kind_mismatch.sc"))
        .output()
        .expect("check mismatched GAT constructor sort");
    assert!(
        !output.status.success(),
        "GAT sort mismatch unexpectedly passed"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("expects sort `access`"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn algebraic_effect_handlers_resume_or_abort_one_shot_continuations() {
    let fixtures = [
        "algebraic_effect_handler.sc",
        "effect_callable_contract.sc",
        "algebraic_effect_abort.sc",
        "algebraic_effect_never_abort.sc",
        "algebraic_effect_function_propagation.sc",
        "algebraic_effect_function_alias.sc",
        "algebraic_effect_static_higher_order.sc",
        "algebraic_effect_reusable_handler.sc",
        "algebraic_effect_reusable_capturing_action.sc",
        "algebraic_effect_reusable_direct_action.sc",
        "algebraic_effect_reusable_ordered_direct_action.sc",
        "algebraic_effect_reusable_borrowed_action.sc",
        "algebraic_effect_erased_callable_forward.sc",
        "algebraic_effect_reusable_fn_mut_action.sc",
        "algebraic_effect_reusable_fn_once_abort.sc",
        "algebraic_effect_reusable_fn_once_resume.sc",
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
        "standard_effect_operations.sc",
        "source_await_handler.sc",
    ];
    for (fixture_name, output) in trapping_fixture_outputs_in_parallel(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{fixture_name}: {}",
            output_text(&output)
        );
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
        .arg(fixture("fail", "algebraic_effect_never_abort_resume.sc"))
        .output()
        .expect("reject a resume parameter on a never-returning operation");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("without `resume`"),
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
fn reusable_handler_action_rejects_overlapping_staged_borrows() {
    let output = salic()
        .arg("check")
        .arg(fixture(
            "fail",
            "algebraic_effect_reusable_borrowed_action_overlap.sc",
        ))
        .output()
        .expect("check overlapping borrowed handler action fixture");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already borrowed"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn erased_effect_callable_rejects_reuse_and_unsupported_shapes() {
    for (name, expected) in [
        ("algebraic_effect_erased_callable_twice.sc", "one-shot"),
        (
            "algebraic_effect_erased_callable_borrow_escape.sc",
            "cannot escape while it captures a borrow",
        ),
        (
            "algebraic_effect_erased_callable_shape.sc",
            "requires one optional input group, move passing, and exactly the handled effect",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid erased effect callable fixture");
        assert!(!output.status.success(), "{name}: {}", output_text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn algebraic_handler_boundaries_report_source_names() {
    for (name, expected) in [
        (
            "algebraic_effect_overload_positional.sc",
            "overloaded effect operation `value` requires named arguments",
        ),
        (
            "algebraic_effect_overload_clause_labels.sc",
            "overloaded handler clause `value` must name the operation parameters in declaration order before `resume`",
        ),
        (
            "algebraic_effect_continuation_escape.sc",
            "continuation `resume` cannot escape its handler clause",
        ),
        (
            "algebraic_effect_function_alias_escape.sc",
            "effectful function alias `action` cannot escape its handler or be used as a runtime value",
        ),
        (
            "algebraic_effect_dynamic_callable_escape.sc",
            "dynamic effectful callable `selected` cannot escape its handler as a runtime value",
        ),
        (
            "algebraic_effect_mutable_function_alias.sc",
            "effectful function alias `action` must be an inferred immutable binding",
        ),
        (
            "algebraic_effect_dynamic_callable_assignment.sc",
            "dynamic effectful callable assignment from `second` to `selected` has an incompatible target set",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check handler rejection boundary");
        assert!(!output.status.success(), "{name}: {}", output_text(&output));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
        assert!(
            !stderr.contains("$handler$"),
            "{name} leaked an internal handler symbol:\n{}",
            output_text(&output)
        );
    }
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
fn classified_documentation_examples_stay_valid() {
    fn markdown_files(directory: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).expect("read documentation directory") {
            let path = entry.expect("read documentation entry").path();
            if path.is_dir() {
                markdown_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("README.md")];
    markdown_files(&root.join("docs"), &mut files);
    files.sort();

    let mut checked = 0;
    let mut fragments = 0;
    for path in files {
        let markdown = fs::read_to_string(&path).expect("read documentation");
        let lines = markdown.lines().collect::<Vec<_>>();
        let mut line = 0;
        while line < lines.len() {
            let Some(classification) = lines[line].strip_prefix("```sc") else {
                line += 1;
                continue;
            };
            let classification = classification.trim();
            assert!(
                matches!(classification, "check" | "fragment" | "future" | "fail"),
                "{}:{} has an unclassified Salicin fence `{}`",
                path.display(),
                line + 1,
                lines[line]
            );
            let start = line + 1;
            line = start;
            while line < lines.len() && lines[line] != "```" {
                line += 1;
            }
            assert!(
                line < lines.len(),
                "{}:{} has an unterminated Salicin fence",
                path.display(),
                start
            );
            let source = lines[start..line].join("\n") + "\n";
            match classification {
                "check" => {
                    let result = if source.contains("let main") {
                        check_source(&source)
                    } else {
                        check_library_source(&source)
                    };
                    assert!(
                        result.is_ok(),
                        "{}:{} documented example failed:\n{}",
                        path.display(),
                        start,
                        result.unwrap_err().join("\n")
                    );
                    checked += 1;
                }
                "fragment" | "fail" => fragments += 1,
                "future" => {}
                _ => unreachable!(),
            }
            line += 1;
        }
    }

    assert!(checked > 0, "no documentation examples are compiled");
    assert!(fragments > 0, "no non-standalone snippets are classified");
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
