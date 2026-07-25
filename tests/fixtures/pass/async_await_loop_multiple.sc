let Poll = std.async.Poll
let Future = std.async.Future

let Step = struct {
  polled: bool,
  remaining: Ptr(mut)(i32),
  drops: Ptr(mut)(i32),
  finish: bool
}

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.drops = *self.drops + 1
  } }
}

extend Step: Future(()) {
  let Output = bool

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(bool) = {
    if self.polled {
      let done = if self.finish {
        unsafe {
          *self.remaining = *self.remaining - 1
          *self.remaining == 0
        }
      } else {
        false
      }
      Poll(bool).Ready(done)
    } else {
      self.polled = true
      Poll(bool).Pending
    }
  }
}

let step(remaining: Ptr(mut)(i32), drops: Ptr(mut)(i32), finish: bool): Step = {
  Step { polled: false, remaining: remaining, drops: drops, finish: finish }
}

let main(): i32 = {
  let mut remaining = 2
  let mut drops = 0
  let remaining_ptr = Ptr(mut)(borrow(mut)(remaining))
  let drops_ptr = Ptr(mut)(borrow(mut)(drops))
  let mut future = async {
    loop {
      let first = await step(remaining_ptr, drops_ptr, false)
      let done = await step(remaining_ptr, drops_ptr, true)
      if done {
        break(if first { 0 } else { 34 })
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
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let fifth = match future.poll()
    { Pending -> 0 }
    { Ready(value) -> value }

  let mut cancel_remaining = 2
  let mut cancel_drops = 0
  let cancel_remaining_ptr = Ptr(mut)(borrow(mut)(cancel_remaining))
  let cancel_drops_ptr = Ptr(mut)(borrow(mut)(cancel_drops))
  do {
    let mut cancelled = async {
      loop {
        let first = await step(cancel_remaining_ptr, cancel_drops_ptr, false)
        let done = await step(cancel_remaining_ptr, cancel_drops_ptr, true)
        if done {
          break()
        } else {
          continue()
        }
      }
    }
    match cancelled.poll()
      { Pending -> () }
      { Ready(_) -> () }
    match cancelled.poll()
      { Pending -> () }
      { Ready(_) -> () }
  }

  first + second + third + fourth + fifth + unsafe { *drops_ptr } +
    unsafe { *cancel_drops_ptr } - 2
}

test("async_await_loop_multiple.sc") {
  main() == 42
}
