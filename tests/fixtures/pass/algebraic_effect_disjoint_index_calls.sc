let Step = effect {
  let delta(): i32
}

let State = struct {
  values: Array(i32)(2),
  drops: MutPtr(i32),
}

extend State: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.drops = *self.drops + 1
  } }
}

let update(left: borrow(mut)(i32), right: borrow(mut)(i32)): () with(Step) = {
  let delta = Step.delta()
  left = left + delta
  right = right + delta
}

let program(drops: MutPtr(i32)): i32 with(Step) = {
  let mut state = State { values: [20, 20], drops: drops }
  update(state.values[0], state.values[1])
  state.values[0] + state.values[1]
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
    40
  } action {
    program(drops)
  }
  let drop_count = unsafe { *drops }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  resumed + abandoned + drop_count - 42
}
