let future = core.async.future
let poll = core.async.poll
let result = core.result
let throwing = core.error.throwing

let step = struct {
  drops: ptr(mut)(i32),
  polls: i32,
  value: i32,
  drop_amount: i32,
}

extend(step, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + self.drop_amount
    }
  }
}

extend(step, future(())) {
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
  fail: bool,
): step with(throwing(bool)) = {
  unsafe {
    *calls = *calls + 1
  }
  if fail {
    throw true
  } else {
    step { drops: drops, polls: 0, value: first + 40, drop_amount: 1 }
  }
}

let run(
  drops: ptr(mut)(i32),
  calls: ptr(mut)(i32),
  fail: bool,
): i32 = {
  let result: result(bool)(i32) = try {
    let mut future = async {
      let first = await step { drops: drops, polls: 0, value: 2, drop_amount: 10 }
      let second = await make_second(drops, calls, first, fail)
      second
    }
    let first = future.poll()
    let second = future.poll()
    if fail {
      0
    } else {
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
  match result
    { ok(value) -> value }
    { err(error) -> if error { 42 } else { 0 } }
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

  let success = run(drops, calls, false)
  let failure = run(drops, calls, true)
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

  if success == 42 && failure == 42 &&
    drop_count == 21 && call_count == 2 {
    42
  } else {
    0
  }
}

test("async_residual_later_await_failure.sc") {
  main() == 42
}
