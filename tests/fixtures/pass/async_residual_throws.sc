let Future = std.async.Future
let Poll = std.async.Poll
let Result = std.Result
let Throws = std.error.Throws

let Resource = struct {
  value: i32,
  drops: Ptr(mut)(i32),
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let choose(fail: bool, value: i32): i32 with(Throws(bool)) = {
  if fail {
    throw(true)
  } else {
    value
  }
}

let consume_or_throw(move resource: Resource): i32 with(Throws(bool)) = {
  choose(true, resource.value)
}

let poll_once(E: effect, F: type, T: type)
  (future: borrow(mut)(F)): Poll(T) with(E)
where F: Future(E, Output = T) = {
  future.poll()
}

let main(): i32 = {
  let offset = 42
  let mut success = async {
    choose(false, offset)
  }
  let success_result: Result(bool)(Poll(i32)) = try {
    success.poll()
  }
  let success_value = match success_result
    { Ok(polled) -> match polled
      { Ready(value) -> value }
      { Pending -> 0 } }
    { Err(_) -> 0 }

  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
  }
  let resource = Resource { value: 2, drops: drops }
  let mut failure = async {
    consume_or_throw(resource)
  }
  let failure_result: Result(bool)(Poll(i32)) = try {
    failure.poll()
  }
  let failed = match failure_result
    { Ok(_) -> false }
    { Err(error) -> error }
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

test("async_residual_throws.sc") {
  main() == 42
}
