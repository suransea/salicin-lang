let future = std.async.future
let poll = std.async.poll
let result = std.result
let throwing = std.error.throwing

let step = struct {
  drops: ptr(mut)(i32),
  done: bool,
}

extend(step, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

extend(step, future(())) {
  let output = bool

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(bool) = {
    poll(bool).ready(self.done)
  }
}

let increment(calls: ptr(mut)(i32)): i32 = {
  unsafe {
    *calls = *calls + 1
    *calls
  }
}

let make_step(
  drops: ptr(mut)(i32),
  calls: ptr(mut)(i32),
  fail_at: i32,
): step with(throwing(bool)) = {
  let call = increment(calls)
  if call == fail_at {
    throw true
  } else {
    step { drops: drops, done: call == 3 }
  }
}

let run(
  drops: ptr(mut)(i32),
  calls: ptr(mut)(i32),
  fail_at: i32,
): i32 = {
  let result: result(bool)(i32) = try {
    let mut future = async {
      loop {
        let done = await make_step(drops, calls, fail_at)
        if done {
          break 42
        } else {
          continue()
        }
      }
    }
    let first = future.poll()
    if fail_at == 0 {
      let second = future.poll()
      match first
        { pending -> match second
          { ready(value) -> value }
          { pending -> 0 } }
        { ready(_) -> 0 }
    } else {
      0
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
  let success_calls = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let failure_calls = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
    *success_calls = 0
    *failure_calls = 0
  }

  let success = run(drops, success_calls, 0)
  let failure = run(drops, failure_calls, 2)
  let drop_count = unsafe {
    *drops
  }
  let success_count = unsafe {
    *success_calls
  }
  let failure_count = unsafe {
    *failure_calls
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
    raw_dealloc(success_calls, size_of(i32), align_of(i32))
    raw_dealloc(failure_calls, size_of(i32), align_of(i32))
  }

  if success == 42 && failure == 42 &&
    drop_count == 4 && success_count == 3 && failure_count == 2 {
    42
  } else {
    0
  }
}

test("async_residual_loop_await_failure.sc") {
  main() == 42
}
