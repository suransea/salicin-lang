let unsafety = core.unsafe.unsafety

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

let update: with(step, unsafety)(
  state: borrow(mut)(state),
  calls: ptr(mut)(i32),
): i32 = {
  let delta = step.delta()
  unsafe { *calls = *calls + 1 }
  state.value = state.value + delta
  state.value
}

let unsafe_outside(
  drops: ptr(mut)(i32),
  calls: ptr(mut)(i32),
  abandon: bool,
): i32 = {
  unsafe {
    let mut state = state { value: 20, drops: drops }
    step.handle delta { (resume) ->
        if abandon { 40 } else { resume(1) }
      } action {
        let value = update(state, calls)
        value + state.value
      }
  }
}

let unsafe_inside(
  drops: ptr(mut)(i32),
  calls: ptr(mut)(i32),
  abandon: bool,
): i32 = {
  let mut state = state { value: 20, drops: drops }
  step.handle delta { (resume) ->
      if abandon { 40 } else { resume(1) }
    } action {
      unsafe {
        let value = update(state, calls)
        value + state.value
      }
    }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let calls = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
    *calls = 0
  }

  let outer_success = unsafe_outside(drops, calls, false)
  let outer_abandon = unsafe_outside(drops, calls, true)
  let inner_success = unsafe_inside(drops, calls, false)
  let inner_abandon = unsafe_inside(drops, calls, true)
  let drop_count = unsafe { *drops }
  let call_count = unsafe { *calls }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
    raw_dealloc(calls, size_of(i32), align_of(i32))
  }
  outer_success + outer_abandon + inner_success + inner_abandon +
    drop_count + call_count - 128
}

test("algebraic_effect_owned_residual_unsafe.sc") {
  std.test.assert(main() == 42)
}
