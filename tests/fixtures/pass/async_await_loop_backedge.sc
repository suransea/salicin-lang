let Poll = std.async.Poll
let Future = std.async.Future

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

let next_step(remaining: Ptr(mut)(i32)): Step = {
  Step { polled: false, remaining: remaining }
}

let ReadyStep = struct {
  remaining: Ptr(mut)(i32)
}

extend ReadyStep: Future(()) {
  let Output = bool

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(bool) = {
    let done = unsafe {
      *self.remaining = *self.remaining - 1
      *self.remaining == 0
    }
    Poll(bool).Ready(done)
  }
}

let ready_step(remaining: Ptr(mut)(i32)): ReadyStep = {
  ReadyStep { remaining: remaining }
}

let main(): i32 = {
  let mut remaining = 3
  let remaining_ptr = Ptr(mut)(borrow(mut)(remaining))
  let mut future = async {
    loop {
      let done = await next_step(remaining_ptr)
      if done {
        break()
      } else {
        continue()
      }
    }
  }

  let first = match future.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let second = match future.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let third = match future.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let fourth = match future.poll()
    { Pending -> 0 }
    { Ready(_) -> 39 }

  let mut immediate_remaining = 3
  let immediate_ptr = Ptr(mut)(borrow(mut)(immediate_remaining))
  let mut immediate = async {
    loop {
      let done = await ready_step(immediate_ptr)
      if done {
        break()
      } else {
        continue()
      }
    }
  }
  let immediate_ready = match immediate.poll()
    { Pending -> 0 }
    { Ready(_) -> 1 }

  first + second + third + fourth + immediate_ready - 1
}

test("async_await_loop_backedge.sc") {
  main() == 42
}
