let Audit = effect {
  let adjust(): i32
}

let Step = effect {
  let delta(): i32
}

let State = struct {
  value: i32,
  drops: Ptr(mut)(i32),
}

extend State: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.drops = *self.drops + 1
  } }
}

let update(state: borrow(mut)(State)): i32 with(Audit, Step) = {
  let adjustment = Audit.adjust()
  let delta = Step.delta()
  state.value = state.value + adjustment + delta
  state.value
}

let audit_outside(
  drops: Ptr(mut)(i32),
  abandon_audit: bool,
  abandon_step: bool,
): i32 = {
  let mut state = State { value: 20, drops: drops }
  Audit.handle adjust { (resume) ->
    if abandon_audit { 40 } else { resume(1) }
  } action {
    Step.handle delta { (resume) ->
      if abandon_step { 40 } else { resume(1) }
    } action {
      let value = update(state)
      value + state.value
    }
  }
}

let step_outside(
  drops: Ptr(mut)(i32),
  abandon_audit: bool,
  abandon_step: bool,
): i32 = {
  let mut state = State { value: 20, drops: drops }
  Step.handle delta { (resume) ->
    if abandon_step { 40 } else { resume(1) }
  } action {
    Audit.handle adjust { (resume) ->
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
