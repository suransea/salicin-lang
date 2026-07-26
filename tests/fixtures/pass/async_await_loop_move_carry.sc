let Poll = std.async.Poll
let Future = std.async.Future

let Resource = struct {
  drops: Ptr(mut)(i32)
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let Step = struct {
  polled: bool,
  remaining: Ptr(mut)(i32)
}

extend Step: Future(()) {
  let Output = bool

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(bool) = {
    if self.polled {
      let done = unsafe {
        *self.remaining = *self.remaining - 1
        *self.remaining == 0
      }
      Poll(bool).Ready(done)
    } else {
      self.polled = true
      Poll(bool).Pending
    }
  }
}

let step(remaining: Ptr(mut)(i32)): Step = {
  Step { polled: false, remaining: remaining }
}

let consume(move first: Resource, move second: Resource): i32 = {
  38
}

let main(): i32 = {
  let mut drops = 0
  let drops_ptr = Ptr(mut)(borrow(mut)(drops))
  let output = do {
    let first_resource = Resource { drops: drops_ptr }
    let second_resource = Resource { drops: drops_ptr }
    let mut remaining = 2
    let remaining_ptr = Ptr(mut)(borrow(mut)(remaining))
    let mut future = async {
      loop {
        let done = await step(remaining_ptr)
        if done {
          break(consume(first_resource, second_resource))
        } else {
          continue()
        }
      }
    }
    match future.poll()
      { Pending -> () }
      { Ready(_) -> () }
    match future.poll()
      { Pending -> () }
      { Ready(_) -> () }
    match future.poll()
      { Pending -> 0 }
      { Ready(value) -> value }
  }

  do {
    let first_resource = Resource { drops: drops_ptr }
    let second_resource = Resource { drops: drops_ptr }
    let mut remaining = 2
    let remaining_ptr = Ptr(mut)(borrow(mut)(remaining))
    let mut cancelled = async {
      loop {
        let done = await step(remaining_ptr)
        if done {
          break(consume(first_resource, second_resource))
        } else {
          continue()
        }
      }
    }
    match cancelled.poll()
      { Pending -> () }
      { Ready(_) -> () }
  }

  output + unsafe { *drops_ptr }
}

test("async_await_loop_move_carry.sc") {
  main() == 42
}
