let future = std.async.future
let poll = std.async.poll
let result = std.result
let throwing = std.error.throwing

let resource = struct {
  value: i32,
  drops: ptr(mut)(i32),
}

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let choose(fail: bool, value: i32): i32 with(throwing(bool)) = {
  if fail {
    throw(true)
  } else {
    value
  }
}

let consume_or_throw(move resource: resource): i32 with(throwing(bool)) = {
  choose(true, resource.value)
}

let poll_once(comptime e: effects, comptime f: type, comptime t: type)
  (future: borrow(mut)(f)): poll(t) with(e)
where f: future(e, output = t) = {
  future.poll()
}

let main(): i32 = {
  let offset = 42
  let mut success = async {
    choose(false, offset)
  }
  let success_result: result(bool)(poll(i32)) = try {
    success.poll()
  }
  let success_value = match success_result
    { ok(polled) -> match polled
      { ready(value) -> value }
      { pending -> 0 } }
    { err(_) -> 0 }

  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
  }
  let resource = resource { value: 2, drops: drops }
  let mut failure = async {
    consume_or_throw(resource)
  }
  let failure_result: result(bool)(poll(i32)) = try {
    failure.poll()
  }
  let failed = match failure_result
    { ok(_) -> false }
    { err(error) -> error }
  let drop_count = unsafe {
    *drops
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }

  if success_value == 42 && failed && drop_count == 1 {
    42
  } else {
    0
  }
}

test("async_residual_failure.sc") {
  main() == 42
}
