let poll = std.async.poll
let future = std.async.future

let step = struct {
  polled: bool,
  remaining: ptr(mut)(i32)
}

extend(step, future(())) {
  let output = bool

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(bool) = {
    if self.polled {
      let done = unsafe {
        *self.remaining = *self.remaining - 1
        *self.remaining == 0
      }
      poll(bool).ready(done)
    } else {
      self.polled = true
      poll(bool).pending
    }
  }
}

let next_step(remaining: ptr(mut)(i32)): step = {
  step { polled: false, remaining: remaining }
}

let ready_step = struct {
  remaining: ptr(mut)(i32)
}

extend(ready_step, future(())) {
  let output = bool

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(bool) = {
    let done = unsafe {
      *self.remaining = *self.remaining - 1
      *self.remaining == 0
    }
    poll(bool).ready(done)
  }
}

let ready_step(remaining: ptr(mut)(i32)): ready_step = {
  ready_step { remaining: remaining }
}

let main(): i32 = {
  let mut remaining = 3
  let remaining_ptr = ptr(mut)(borrow(mut)(remaining))
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
    { pending -> 1 }
    { ready(_) -> 0 }
  let second = match future.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let third = match future.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let fourth = match future.poll()
    { pending -> 0 }
    { ready(_) -> 39 }

  let mut immediate_remaining = 3
  let immediate_ptr = ptr(mut)(borrow(mut)(immediate_remaining))
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
    { pending -> 0 }
    { ready(_) -> 1 }

  first + second + third + fourth + immediate_ready - 1
}

test("async_await_loop_backedge.sc") {
  main() == 42
}
