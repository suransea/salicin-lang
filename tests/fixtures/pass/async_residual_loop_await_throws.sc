let Future = std.async.Future
let Poll = std.async.Poll
let Result = std.Result
let Throws = std.error.Throws

let Step = struct {
  drops: Ptr(mut)(i32),
  done: bool,
}

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

extend Step: Future(()) {
  let Output = bool

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(bool) = {
    Poll(bool).Ready(self.done)
  }
}

let increment(calls: Ptr(mut)(i32)): i32 = {
  unsafe {
    *calls = *calls + 1
    *calls
  }
}

let make_step(
  drops: Ptr(mut)(i32),
  calls: Ptr(mut)(i32),
  fail_at: i32,
): Step with(Throws(bool)) = {
  let call = increment(calls)
  if call == fail_at {
    throw true
  } else {
    Step { drops: drops, done: call == 3 }
  }
}

let run(
  drops: Ptr(mut)(i32),
  calls: Ptr(mut)(i32),
  fail_at: i32,
): i32 = {
  let result: Result(bool)(i32) = try {
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
        { Pending -> match second
          { Ready(value) -> value }
          { Pending -> 0 } }
        { Ready(_) -> 0 }
    } else {
      0
    }
  }
  match result
    { Ok(value) -> value }
    { Err(error) -> if error { 42 } else { 0 } }
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

test("async_residual_loop_await_throws.sc") {
  main() == 42
}
