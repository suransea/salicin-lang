let future = std.async.future
let poll = std.async.poll
let result = std.result
let throws = std.error.throws

let resource = struct {
  drops: ptr(mut)(i32),
}

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let step = struct {
  polls: i32,
  value: i32,
  resource: resource,
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

let choose(fail: bool): i32 with(throws(bool)) = {
  if fail {
    throw true
  } else {
    40
  }
}

let make_step(move resource: resource, fail: bool): step with(throws(bool)) = {
  step { polls: 0, value: choose(fail), resource: resource }
}

let run_success(drops: ptr(mut)(i32)): i32 = {
  let result: result(bool)(i32) = try {
    let resource = resource { drops: drops }
    let mut future = async {
      await make_step(resource, false)
    }
    let first = future.poll()
    let second = future.poll()
    match first
      { pending -> match second
        { ready(value) -> value }
        { pending -> 0 } }
      { ready(_) -> 0 }
  }

  match result
    { ok(value) -> if value == 40 { 42 } else { 0 } }
    { err(_) -> 0 }
}

let run_failure(drops: ptr(mut)(i32)): i32 = {
  let result: result(bool)(i32) = try {
    let resource = resource { drops: drops }
    let mut future = async {
      await make_step(resource, true)
    }
    let first = future.poll()
    let second = future.poll()
    match first
      { pending -> match second
        { ready(value) -> value }
        { pending -> 0 } }
      { ready(_) -> 0 }
  }

  match result
    { ok(_) -> 0 }
    { err(error) -> if error { 42 } else { 0 } }
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
