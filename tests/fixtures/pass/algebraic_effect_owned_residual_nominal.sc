let audit = effect {
  let adjust(): i32
}

let step = effect {
  let delta(): i32
}

let state = struct {
  value: i32,
  drops: ptr(mut)(i32),
}

extend(state, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let update(state: borrow(mut)(state)): i32 with(audit, step) = {
  let adjustment = audit.adjust()
  let delta = step.delta()
  state.value = state.value + adjustment + delta
  state.value
}

let audit_outside(
  drops: ptr(mut)(i32),
  abandon_audit: bool,
  abandon_step: bool,
): i32 = {
  let mut state = state { value: 20, drops: drops }
  audit.handle adjust { (resume) ->
      if abandon_audit { 40 } else { resume(1) }
    } action {
      step.handle delta { (resume) ->
        if abandon_step { 40 } else { resume(1) }
      } action {
        let value = update(state)
        value + state.value
      }
    }
}

let step_outside(
  drops: ptr(mut)(i32),
  abandon_audit: bool,
  abandon_step: bool,
): i32 = {
  let mut state = state { value: 20, drops: drops }
  step.handle delta { (resume) ->
      if abandon_step { 40 } else { resume(1) }
    } action {
      audit.handle adjust { (resume) ->
        if abandon_audit { 40 } else { resume(1) }
      } action {
        let value = update(state)
        value + state.value
      }
    }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let audit_success = audit_outside(drops, false, false)
  let audit_abandon = audit_outside(drops, true, false)
  let step_abandon = audit_outside(drops, false, true)
  let reverse_success = step_outside(drops, false, false)
  let reverse_audit_abandon = step_outside(drops, true, false)
  let reverse_step_abandon = step_outside(drops, false, true)
  let drop_count = unsafe { *drops }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  audit_success + audit_abandon + step_abandon + reverse_success +
    reverse_audit_abandon + reverse_step_abandon + drop_count - 212
}

test("algebraic_effect_owned_residual_nominal.sc") {
  main() == 42
}
