let Future = std.async.Future
let Poll = std.async.Poll
let Result = std.Result
let Throws = std.error.Throws

let Resource = struct {
  drops: Ptr(mut)(i32),
  value: i32,
}

let Step = struct {
  drops: Ptr(mut)(i32),
  polls: i32,
  value: i32,
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 100
    }
  }
}

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 10
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

let finish(
  calls: Ptr(mut)(i32),
  fail: bool,
  value: i32,
): i32 with(Throws(bool)) = {
  unsafe {
    *calls = *calls + 1
  }
  if fail {
    throw true
  } else {
    value + 1
  }
}

let run(
  drops: Ptr(mut)(i32),
  calls: Ptr(mut)(i32),
  fail: bool,
): i32 = {
  let result: Result(bool)(i32) = try {
    let mut future = async {
      let retained = Resource { drops: drops, value: 1 }
      let value = await Step { drops: drops, polls: 0, value: 40 }
      let completed = value + retained.value
      finish(calls, fail, completed)
    }
    let pending = future.poll()
    let ready = future.poll()
    match pending
      { Pending -> match ready
        { Ready(value) -> value }
        { Pending -> 0 } }
      { Ready(_) -> 0 }
  }
  match result
    { Ok(value) -> value }
    { Err(error) -> if error { 42 } else { 0 } }
}

let run_cancelled(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  let result: Result(bool)(i32) = try {
    let mut future = async {
      let retained = Resource { drops: drops, value: 1 }
      let value = await Step { drops: drops, polls: 0, value: 40 }
      let completed = value + retained.value
      finish(calls, false, completed)
    }
    match future.poll()
      { Pending -> 42 }
      { Ready(_) -> 0 }
  }
  match result
    { Ok(value) -> value }
    { Err(_) -> 0 }
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
  let cancelled = run_cancelled(drops, calls)
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

  if success == 42 && failure == 42 && cancelled == 42 &&
    drop_count == 330 && call_count == 2 {
    42
  } else {
    0
  }
}

test("async_residual_later_throws.sc") {
  main() == 42
}
