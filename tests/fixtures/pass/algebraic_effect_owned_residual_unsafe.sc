let Unsafe = std.effect.Unsafe

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

let update(
  state: borrow(mut)(State),
  calls: Ptr(mut)(i32),
): i32 with(Step, Unsafe) = {
  let delta = Step.delta()
  unsafe { *calls = *calls + 1 }
  state.value = state.value + delta
  state.value
}

let unsafe_outside(
  drops: Ptr(mut)(i32),
  calls: Ptr(mut)(i32),
  abandon: bool,
): i32 = { unsafe {
  let mut state = State { value: 20, drops: drops }
  Step.handle delta { (resume) ->
    if abandon { 40 } else { resume(1) }
  } action {
    let value = update(state, calls)
    value + state.value
  }
} }

let unsafe_inside(
  drops: Ptr(mut)(i32),
  calls: Ptr(mut)(i32),
  abandon: bool,
): i32 = {
  let mut state = State { value: 20, drops: drops }
  Step.handle delta { (resume) ->
    if abandon { 40 } else { resume(1) }
  } action { unsafe {
    let value = update(state, calls)
    value + state.value
  } }
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
