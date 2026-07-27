let result = std.result
let throwing = std.error.throwing

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

let accept(fail: bool): i32 with(throwing(bool)) = {
  if fail { throw(true) } else { 0 }
}

let update(state: borrow(mut)(state), fail: bool): i32 with(step, throwing(bool)) = {
  let accepted = accept(fail)
  let delta = step.delta()
  state.value = state.value + delta
  state.value + accepted
}

let run(drops: ptr(mut)(i32), fail: bool): i32 with(throwing(bool)) = {
  let mut state = state { value: 20, drops: drops }
  step.handle delta { (resume) ->
      resume(1)
    } action {
      update(state, fail)
    }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let success: result(bool)(i32) = try { run(drops, false) }
  let failure: result(bool)(i32) = try { run(drops, true) }
  let drop_count = unsafe { *drops }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  (success ?? 0) + (failure ?? 5) + drop_count + 14
}

test("algebraic_effect_owned_residual_failure_outer.sc") {
  main() == 42
}
