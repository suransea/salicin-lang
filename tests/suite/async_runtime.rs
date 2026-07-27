use crate::support::*;

#[test]
fn cold_async_state_owns_and_drops_unpolled_captures() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_cold_cancel.sc"))
        .output()
        .expect("run cold async cancellation fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn cold_async_future_polls_once_and_rejects_repolling() {
    let ready = salic()
        .arg("run")
        .arg(fixture("pass", "async_ready_poll.sc"))
        .output()
        .expect("run ready async polling fixture");
    assert_eq!(ready.status.code(), Some(42), "{}", output_text(&ready));

    let repolled = salic()
        .arg("run")
        .arg(fixture("pass", "async_repoll_trap.sc"))
        .output()
        .expect("run completed future repoll fixture");
    assert!(
        !repolled.status.success(),
        "completed future unexpectedly allowed a second poll: {}",
        output_text(&repolled)
    );
}

#[test]
fn spin_executor_polls_one_future_until_ready() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_spin_executor.sc"))
        .output()
        .expect("run spin executor fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn copy_capturing_async_residual_effect_specializes_under_handler() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_residual_effect.sc"))
        .output()
        .expect("run residual-effect async fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn move_capturing_async_residual_effect_specializes_under_handler() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_residual_move_capture.sc"))
        .output()
        .expect("run move-capturing residual-effect async fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn borrowed_async_residual_effect_captures_specialize_under_handler() {
    for (name, output) in batched_native_fixture_outputs(&[
        "async_residual_borrow_capture.sc",
        "async_residual_mut_borrow_capture.sc",
    ]) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let conflict = salic()
        .arg("check")
        .arg(fixture(
            "fail",
            "async_residual_mut_borrow_capture_conflict.sc",
        ))
        .output()
        .expect("check mutable async capture conflict");
    assert!(!conflict.status.success(), "{}", output_text(&conflict));
    assert!(
        String::from_utf8_lossy(&conflict.stderr).contains("borrowed"),
        "{}",
        output_text(&conflict)
    );
}

#[test]
fn ready_tail_await_and_post_await_async_failure_specialize_under_try() {
    for (name, output) in batched_native_fixture_outputs(&[
        "async_residual_failure.sc",
        "async_residual_failure_tail_await.sc",
        "async_residual_failure_await.sc",
        "async_residual_later_failure.sc",
        "async_residual_later_await_failure.sc",
        "async_residual_loop_await_failure.sc",
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
fn suspended_residual_suspensions_specialize_and_cancel() {
    for (name, output) in batched_native_fixture_outputs(&[
        "async_residual_tail_await.sc",
        "async_residual_post_await.sc",
        "async_residual_retained_await.sc",
        "async_residual_nested_await.sc",
        "async_residual_borrow_await.sc",
        "async_residual_branch_await.sc",
        "async_residual_heterogeneous_branch.sc",
        "async_residual_later_effect.sc",
        "async_residual_later_await.sc",
        "async_residual_loop_await.sc",
        "async_residual_while_await.sc",
    ]) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let conflict = salic()
        .arg("check")
        .arg(fixture("fail", "async_residual_borrow_await_conflict.sc"))
        .output()
        .expect("check suspended residual borrow conflict");
    assert!(!conflict.status.success(), "{}", output_text(&conflict));
    assert!(
        String::from_utf8_lossy(&conflict.stderr).contains("borrowed"),
        "{}",
        output_text(&conflict)
    );

    let self_reference = salic()
        .arg("check")
        .arg(fixture(
            "fail",
            "async_residual_heterogeneous_wrapped_self_reference.sc",
        ))
        .output()
        .expect("check wrapped heterogeneous residual self-reference");
    assert!(
        !self_reference.status.success(),
        "{}",
        output_text(&self_reference)
    );
    assert!(
        String::from_utf8_lossy(&self_reference.stderr)
            .contains("self-referential and cannot implement `movable`"),
        "{}",
        output_text(&self_reference)
    );

    let multiple = salic()
        .arg("check")
        .arg(fixture("fail", "async_residual_loop_multiple_await.sc"))
        .output()
        .expect("check multiple-await recurring residual async loop");
    assert!(!multiple.status.success(), "{}", output_text(&multiple));
    assert!(
        String::from_utf8_lossy(&multiple.stderr).contains(
            "await residual failure and algebraic effects require poll/resume handler specialization"
        ),
        "{}",
        output_text(&multiple)
    );
}

#[test]
fn tail_await_forwards_a_ready_child_future() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_ready.sc"))
        .output()
        .expect("run async await fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn non_tail_await_resumes_after_a_pending_child_poll() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_pending.sc"))
        .output()
        .expect("run pending async await fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn cancelling_a_suspended_await_drops_the_child_future_once() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_cancel.sc"))
        .output()
        .expect("run suspended async cancellation fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn completing_an_await_drops_the_child_future_once() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_complete_drop.sc"))
        .output()
        .expect("run completed async child cleanup fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn multiple_sequential_awaits_resume_through_nested_segments() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_multiple.sc"))
        .output()
        .expect("run multiple async await fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn await_retains_preceding_locals_and_external_borrows() {
    for fixture_name in [
        "async_await_retains_local.sc",
        "async_await_external_borrow.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run async retained state fixture");
        assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
    }
}

#[test]
fn await_drops_a_retained_resource_after_ready() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_retained_resource_drop.sc"))
        .output()
        .expect("run retained async resource fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn await_hoists_over_if_and_match_branches() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_control_branches.sc"))
        .output()
        .expect("run async control-flow await fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn await_hoists_out_of_loops_that_exit_on_the_first_iteration() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_terminating_loops.sc"))
        .output()
        .expect("run terminating async loop await fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn await_reuses_one_child_slot_across_loop_backedges() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_loop_backedge.sc"))
        .output()
        .expect("run async loop backedge fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn async_loop_backedges_preserve_value_producing_breaks() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_loop_value.sc"))
        .output()
        .expect("run value-producing async loop fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn async_loop_break_transfers_a_move_only_output_once() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_loop_value_move.sc"))
        .output()
        .expect("run move-only async loop output fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn async_loop_fallthrough_reuses_the_iteration_child() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_loop_fallthrough.sc"))
        .output()
        .expect("run async loop fallthrough fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn recurring_async_while_rechecks_pre_and_post_test_conditions() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_recurring_while.sc"))
        .output()
        .expect("run recurring async while fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn recurring_async_loop_composes_multiple_iteration_awaits() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_loop_multiple.sc"))
        .output()
        .expect("run multiple-await async loop fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn recurring_async_loop_transfers_move_only_carry() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_loop_move_carry.sc"))
        .output()
        .expect("run move-only async loop carry fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn recurring_async_loop_without_break_has_never_output() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_infinite_loop.sc"))
        .output()
        .expect("run unpolled infinite async loop fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn recurring_async_loop_selects_branch_local_children() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_loop_branches.sc"))
        .output()
        .expect("run branch-local async loop fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn recurring_async_loop_rewrites_nested_iteration_control() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_loop_nested_control.sc"))
        .output()
        .expect("run nested-control async loop fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn cancelling_an_async_loop_drops_completed_and_active_children_once() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_loop_cancel.sc"))
        .output()
        .expect("run async loop cancellation fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn cancelling_a_control_flow_await_drops_only_the_selected_child() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_control_cancel.sc"))
        .output()
        .expect("run async control-flow cancellation fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn cancelling_multiple_awaits_drops_only_the_active_segment_state() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "async_await_multiple_cancel.sc"))
        .output()
        .expect("run multiple async cancellation fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}
