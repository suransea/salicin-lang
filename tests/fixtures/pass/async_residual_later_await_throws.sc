let Future = std.async.Future
let Poll = std.async.Poll
let Result = std.Result
let Throws = std.error.Throws

let Step = struct {
  drops: Ptr(mut)(i32),
  polls: i32,
  value: i32,
  drop_amount: i32,
}

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + self.drop_amount
    }
  }
}

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    if self.polls == 0 {
      self.polls = 1
      Poll(i32).Pending
    } else {
      Poll(i32).Ready(self.value)
    }
  }
}

let make_second(
  drops: Ptr(mut)(i32),
  calls: Ptr(mut)(i32),
  first: i32,
  fail: bool,
): Step with(Throws(bool)) = {
  unsafe {
    *calls = *calls + 1
  }
  if fail {
    throw true
  } else {
    Step { drops: drops, polls: 0, value: first + 40, drop_amount: 1 }
  }
}

let run(
  drops: Ptr(mut)(i32),
  calls: Ptr(mut)(i32),
  fail: bool,
): i32 = {
  let result: Result(bool)(i32) = try {
    let mut future = async {
      let first = await Step { drops: drops, polls: 0, value: 2, drop_amount: 10 }
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
        { Pending -> match second
          { Pending -> match third
            { Ready(value) -> value }
            { Pending -> 0 } }
          { Ready(_) -> 0 } }
        { Ready(_) -> 0 }
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

test("async_residual_later_await_throws.sc") {
  main() == 42
}
