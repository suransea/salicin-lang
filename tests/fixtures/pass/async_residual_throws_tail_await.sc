let Future = std.async.Future
let Poll = std.async.Poll
let Result = std.Result
let Throws = std.error.Throws

let Resource = struct {
  drops: Ptr(mut)(i32),
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let Step = struct {
  polls: i32,
  value: i32,
  resource: Resource,
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

let choose(fail: bool): i32 with(Throws(bool)) = {
  if fail {
    throw true
  } else {
    40
  }
}

let make_step(move resource: Resource, fail: bool): Step with(Throws(bool)) = {
  Step { polls: 0, value: choose(fail), resource: resource }
}

let run_success(drops: Ptr(mut)(i32)): i32 = {
  let result: Result(bool)(i32) = try {
    let resource = Resource { drops: drops }
    let mut future = async {
      await make_step(resource, false)
    }
    let first = future.poll()
    let second = future.poll()
    match first
      { Pending -> match second
        { Ready(value) -> value }
        { Pending -> 0 } }
      { Ready(_) -> 0 }
  }

  match result
    { Ok(value) -> if value == 40 { 42 } else { 0 } }
    { Err(_) -> 0 }
}

let run_failure(drops: Ptr(mut)(i32)): i32 = {
  let result: Result(bool)(i32) = try {
    let resource = Resource { drops: drops }
    let mut future = async {
      await make_step(resource, true)
    }
    let first = future.poll()
    let second = future.poll()
    match first
      { Pending -> match second
        { Ready(value) -> value }
        { Pending -> 0 } }
      { Ready(_) -> 0 }
  }

  match result
    { Ok(_) -> 0 }
    { Err(error) -> if error { 42 } else { 0 } }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
  }

  let success = run_success(drops)
  let failure = run_failure(drops)
  let drop_count = unsafe {
    *drops
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }

  if success == 42 && failure == 42 && drop_count == 2 { 42 } else { 0 }
}

test("async_residual_throws_tail_await.sc") {
  main() == 42
}
