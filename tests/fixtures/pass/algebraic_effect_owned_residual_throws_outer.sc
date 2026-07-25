let Result = std.Result
let Throws = std.error.Throws

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

let accept(fail: bool): i32 with(Throws(bool)) = {
  if fail { throw(true) } else { 0 }
}

let update(state: borrow(mut)(State), fail: bool): i32 with(Step, Throws(bool)) = {
  let accepted = accept(fail)
  let delta = Step.delta()
  state.value = state.value + delta
  state.value + accepted
}

let run(drops: Ptr(mut)(i32), fail: bool): i32 with(Throws(bool)) = {
  let mut state = State { value: 20, drops: drops }
  Step.handle delta { (resume) ->
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

  let success: Result(bool)(i32) = try { run(drops, false) }
  let failure: Result(bool)(i32) = try { run(drops, true) }
  let drop_count = unsafe { *drops }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  (success ?? 0) + (failure ?? 5) + drop_count + 14
}

test("algebraic_effect_owned_residual_throws_outer.sc") {
  main() == 42
}
