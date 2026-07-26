let Step = effect {
  let delta(): i32
}

let Counter = struct {
  value: i32,
}

let State = struct {
  counter: Counter,
  drops: Ptr(mut)(i32),
}

extend State: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let update(value: borrow(mut)(i32)): () with(Step) = {
  let delta = Step.delta()
  value = value + delta
}

let program(drops: Ptr(mut)(i32)): i32 with(Step) = {
  let mut state = State { counter: Counter { value: 40 }, drops: drops }
  update(state.counter.value)
  update(state.counter.value)
  state.counter.value
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

test("algebraic_effect_owned_field_calls.sc") {
  main() == 42
}
