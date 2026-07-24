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

let mark(calls: MutPtr(i32))(digit: i32): i32 = { unsafe {
  *calls = *calls * 10 + digit
  0
} }

let next_index(calls: MutPtr(i32)): i32 = { unsafe {
  *calls = *calls * 10 + 2
  1
} }

let update(before: i32)(value: borrow(mut)(i32))(after: i32): () with(Step) = {
  let delta = Step.delta()
  value = value + delta + before + after
}

let program(drops: MutPtr(i32))(calls: MutPtr(i32)): i32 with(Step) = {
  let mut state = State { values: [0, 40], drops: drops }
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

  let resumed = Step.handle delta { (resume) ->
    resume(1)
  } action {
    program(drops)(calls)
  }
  let abandoned = Step.handle delta { (_) ->
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
