let future = std.async.future
let poll = std.async.poll

let ask = effect {
  let ask(): i32
}

let choice = enum {
  use_first(i32),
  use_second(i32),
}

let first = struct {
  drops: ptr(mut)(i32),
  polls: i32,
  value: i32,
}

let second = struct {
  drops: ptr(mut)(i32),
  polls: i32,
  value: i32,
}

let retained = struct {
  drops: ptr(mut)(i32),
  offset: i32,
}

extend first: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 10
    }
  }
}

extend second: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

extend retained: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 100
    }
  }
}

extend first: future(()) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    if self.polls == 0 {
      self.polls = 1
      poll(i32).pending
    } else {
      poll(i32).ready(self.value)
    }
  }
}

extend second: future(()) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    if self.polls == 0 {
      self.polls = 1
      poll(i32).pending
    } else {
      poll(i32).ready(self.value)
    }
  }
}

let run(drops: ptr(mut)(i32), first: bool): i32 = {
  let mut future = async {
    if first {
      await first { drops: drops, polls: 0, value: ask.ask() }
    } else {
      await second { drops: drops, polls: 0, value: ask.ask() }
    }
  }
  ask.handle ask { (resume) -> resume(40) } action {
      let pending = future.poll()
      let ready = future.poll()
      match pending
        { pending -> match ready
          { ready(value) -> value }
          { pending -> 0 } }
        { ready(_) -> 0 }
    }
}

let cancel(drops: ptr(mut)(i32)): i32 = {
  ask.handle ask { (resume) -> resume(40) } action {
      let mut future = async {
        if false {
          await first { drops: drops, polls: 0, value: ask.ask() }
        } else {
          await second { drops: drops, polls: 0, value: ask.ask() }
        }
      }
      match future.poll()
        { pending -> 42 }
        { ready(_) -> 0 }
    }
}

let run_match(drops: ptr(mut)(i32), move choice: choice): i32 = {
  let mut future = async {
    match choice
      { use_first(offset) ->
        await first { drops: drops, polls: 0, value: ask.ask() + offset } }
      { use_second(offset) ->
        await second { drops: drops, polls: 0, value: ask.ask() + offset } }
  }
  ask.handle ask { (resume) -> resume(40) } action {
      let pending = future.poll()
      let ready = future.poll()
      match pending
        { pending -> match ready
          { ready(value) -> value }
          { pending -> 0 } }
        { ready(_) -> 0 }
    }
}

let run_wrapped(drops: ptr(mut)(i32), move choice: choice): i32 = {
  let mut future = async {
    let retained = retained { drops: drops, offset: 2 }
    let value = await match choice
      { use_first(_) ->
        first { drops: drops, polls: 0, value: ask.ask() } }
      { use_second(_) ->
        second { drops: drops, polls: 0, value: ask.ask() } }
    value + retained.offset
  }
  ask.handle ask { (resume) -> resume(40) } action {
      let pending = future.poll()
      let ready = future.poll()
      match pending
        { pending -> match ready
          { ready(value) -> value }
          { pending -> 0 } }
        { ready(_) -> 0 }
    }
}

let cancel_wrapped(drops: ptr(mut)(i32)): i32 = {
  ask.handle ask { (resume) -> resume(40) } action {
      let mut future = async {
        let retained = retained { drops: drops, offset: 2 }
        let value = await if false {
          first { drops: drops, polls: 0, value: ask.ask() }
        } else {
          second { drops: drops, polls: 0, value: ask.ask() }
        }
        value + retained.offset
      }
      match future.poll()
        { pending -> 42 }
        { ready(_) -> 0 }
    }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
  }

  let first = run(drops, true)
  let second = run(drops, false)
  let matched_first = run_match(drops, choice.use_first(2))
  let matched_second = run_match(drops, choice.use_second(2))
  let wrapped_first = run_wrapped(drops, choice.use_first(0))
  let wrapped_second = run_wrapped(drops, choice.use_second(0))
  let wrapped_cancelled = cancel_wrapped(drops)
  let cancelled = cancel(drops)
  let drop_count = unsafe {
    *drops
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }

  if first == 40 && second == 40 && matched_first == 42 && matched_second == 42 && wrapped_first == 42 && wrapped_second == 42 && wrapped_cancelled == 42 && cancelled == 42 && drop_count == 335 {
    42
  } else {
    0
  }
}

test("async_residual_heterogeneous_branch.sc") {
  main() == 42
}
