let step = effect {
  let delta(): i32
}

let state = struct {
  values: array(i32)(2),
  drops: ptr(mut)(i32),
}

extend state: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let mark(calls: ptr(mut)(i32))(digit: i32): i32 = {
  unsafe {
    *calls = *calls * 10 + digit
    0
  }
}

let next_index(calls: ptr(mut)(i32)): i32 = {
  unsafe {
    *calls = *calls * 10 + 2
    1
  }
}

let update(before: i32)(value: borrow(mut)(i32))(after: i32): () with(step) = {
  let delta = step.delta()
  value = value + delta + before + after
}

let program(drops: ptr(mut)(i32))(calls: ptr(mut)(i32)): i32 with(step) = {
  let mut state = state { values: [0, 40], drops: drops }
  update(mark(calls)(1))(state.values[next_index(calls)])(mark(calls)(3))
  state.values[1]
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

  let resumed = step.handle delta { (resume) ->
      resume(1)
    } action {
      program(drops)(calls)
    }
  let abandoned = step.handle delta { (_) ->
      40
    } action {
      program(drops)(calls)
    }
  let drop_count = unsafe { *drops }
  let argument_order = unsafe { *calls }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
    raw_dealloc(calls, size_of(i32), align_of(i32))
  }
  resumed + abandoned + drop_count + argument_order - 123164
}

test("algebraic_effect_owned_index_calls.sc") {
  main() == 42
}
