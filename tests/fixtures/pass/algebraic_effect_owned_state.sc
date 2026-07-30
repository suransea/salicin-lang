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

extend(state) {
  let add(self: borrow(mut)(self))(amount: i32): () = {
    self.value = self.value + amount
  }
}

let program: with(step)(drops: ptr(mut)(i32)): i32 = {
  let mut state = state { value: 40, drops: drops }
  state.add(1)
  let delta = step.delta()
  state.value = state.value + delta
  let second_delta = step.delta()
  state.value = state.value + second_delta
  state.value
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let resumed = step.handle delta { (resume) ->
      resume(1)
    } action {
      program(drops)
    }
  let abandoned = step.handle delta { (_) ->
      42
    } action {
      program(drops)
    }
  let drop_count = unsafe { *drops }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  resumed + abandoned + drop_count - 45
}

test("algebraic_effect_owned_state.sc") {
  std.test.assert(main() == 42)
}
