let step = effect {
  let delta(): i32
}

let counter = struct {
  value: i32,
}

let state = struct {
  counter: counter,
  drops: ptr(mut)(i32),
}

extend(state, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let update: with(step)(value: borrow(mut)(i32)): () = {
  let delta = step.delta()
  value = value + delta
}

let program: with(step)(drops: ptr(mut)(i32)): i32 = {
  let mut state = state { counter: counter { value: 40 }, drops: drops }
  update(state.counter.value)
  update(state.counter.value)
  state.counter.value
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

test("algebraic_effect_owned_field_calls.sc") {
  main() == 42
}
