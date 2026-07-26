let future = std.async.future
let poll = std.async.poll
let result = std.result
let throws = std.error.throws

let resource = struct {
  drops: ptr(mut)(i32),
  value: i32,
}

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let step = struct {
  polls: i32,
  value: i32,
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

let make_step(fail: bool): step with(throws(bool)) = {
  if fail {
    throw true
  } else {
    step { polls: 0, value: 2 }
  }
}

let run_success(drops: ptr(mut)(i32)): i32 = {
  let result: result(bool)(i32) = try {
    let outer = resource { drops: drops, value: 1 }
    let mut future = async {
      let inner = resource { drops: drops, value: 39 }
      let value = await make_step(false)
      outer.value + inner.value + value
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
    { ok(value) -> value }
    { err(_) -> 0 }
}

let run_failure(drops: ptr(mut)(i32)): i32 = {
  let result: result(bool)(i32) = try {
    let outer = resource { drops: drops, value: 1 }
    let mut future = async {
      let inner = resource { drops: drops, value: 39 }
      let value = await make_step(true)
      outer.value + inner.value + value
    }
    match future.poll()
      { pending -> 0 }
      { ready(value) -> value }
  }
  match result
    { ok(_) -> 0 }
    { err(error) -> if error { 42 } else { 0 } }
}

let run_cancelled(drops: ptr(mut)(i32)): i32 = {
  let result: result(bool)(i32) = try {
    let outer = resource { drops: drops, value: 1 }
    let mut future = async {
      let inner = resource { drops: drops, value: 39 }
      let value = await make_step(false)
      outer.value + inner.value + value
    }
    match future.poll()
      { pending -> 42 }
      { ready(_) -> 0 }
  }
  match result
    { ok(value) -> value }
    { err(_) -> 0 }
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
  let cancelled = run_cancelled(drops)
  let drop_count = unsafe {
    *drops
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }

  if success == 42 && failure == 42 && cancelled == 42 && drop_count == 6 {
    42
  } else {
    0
  }
}

test("async_residual_retained_await.sc") {
  main() == 42
}
