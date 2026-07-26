let future = std.async.future
let poll = std.async.poll

let ask = effect {
  let ask(): i32
}

let step = struct {
  drops: ptr(mut)(i32),
  polls: i32,
  value: i32,
  drop_amount: i32,
}

extend step: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + self.drop_amount
    }
  }
}

extend step: future(()) {
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

let make_second(
  drops: ptr(mut)(i32),
  calls: ptr(mut)(i32),
  first: i32,
): step with(ask) = {
  unsafe {
    *calls = *calls + 1
  }
  step { drops: drops, polls: 0, value: first + ask.ask(), drop_amount: 1 }
}

let abandon(calls: ptr(mut)(i32)): i32 = {
  unsafe {
    *calls = *calls + 1
  }
  42
}

let run_success(drops: ptr(mut)(i32), calls: ptr(mut)(i32)): i32 = {
  let mut future = async {
    let first = await step { drops: drops, polls: 0, value: 2, drop_amount: 10 }
    let second = await make_second(drops, calls, first)
    second
  }
  ask.handle ask { (resume) -> resume(40) } action {
      let first = future.poll()
      let second = future.poll()
      let third = future.poll()
      match first
        { pending -> match second
          { pending -> match third
            { ready(value) -> value }
            { pending -> 0 } }
          { ready(_) -> 0 } }
        { ready(_) -> 0 }
    }
}

let run_cancelled(drops: ptr(mut)(i32), calls: ptr(mut)(i32)): i32 = {
  ask.handle ask { (resume) -> resume(40) } action {
      let mut future = async {
        let first = await step { drops: drops, polls: 0, value: 2, drop_amount: 10 }
        let second = await make_second(drops, calls, first)
        second
      }
      let first = future.poll()
      let second = future.poll()
      match first
        { pending -> match second
          { pending -> 42 }
          { ready(_) -> 0 } }
        { ready(_) -> 0 }
    }
}

let run_abandoned(drops: ptr(mut)(i32), calls: ptr(mut)(i32)): i32 = {
  ask.handle ask { (_) -> abandon(calls) } action {
      let mut future = async {
        let first = await step { drops: drops, polls: 0, value: 2, drop_amount: 10 }
        let second = await make_second(drops, calls, first)
        second
      }
      let first = future.poll()
      let second = future.poll()
      match first
        { pending -> match second
          { pending -> 0 }
          { ready(_) -> 0 } }
        { ready(_) -> 0 }
    }
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

  let success = run_success(drops, calls)
  let cancelled = run_cancelled(drops, calls)
  let abandoned = run_abandoned(drops, calls)
  let drop_count = unsafe {
    *drops
  }
  let call_count = unsafe {
    *calls
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
    raw_dealloc(calls, size_of(i32), align_of(i32))
  }

  if success == 42 && cancelled == 42 && abandoned == 42 &&
    drop_count == 32 && call_count == 4 {
    42
  } else {
    0
  }
}

test("async_residual_later_await.sc") {
  main() == 42
}
