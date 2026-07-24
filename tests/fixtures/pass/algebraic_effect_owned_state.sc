let Step = effect {
  let delta(): i32
}

let State = struct {
  value: i32,
  drops: MutPtr(i32),
}

extend State: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.drops = *self.drops + 1
  } }
}

extend State {
  let add(self: borrow(mut)(Self))(amount: i32): () = {
    self.value = self.value + amount
  }
}

let program(drops: MutPtr(i32)): i32 with(Step) = {
  let mut state = State { value: 40, drops: drops }
  state.add(1)
  let delta = Step.delta()
  state.value = state.value + delta
  let second_delta = Step.delta()
  state.value = state.value + second_delta
  state.value
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let resumed = Step.handle delta { (resume) ->
    resume(1)
  } action {
    program(drops)
  }
  let abandoned = Step.handle delta { (_) ->
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
