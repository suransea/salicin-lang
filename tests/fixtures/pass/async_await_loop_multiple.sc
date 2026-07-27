let poll = std.async.poll
let future = std.async.future

let step = struct {
  polled: bool,
  remaining: ptr(mut)(i32),
  drops: ptr(mut)(i32),
  finish: bool
}

extend(step, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

extend(step, future(())) {
  let output = bool

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(bool) = {
    if self.polled {
      let done = if self.finish {
        unsafe {
          *self.remaining = *self.remaining - 1
          *self.remaining == 0
        }
      } else {
        false
      }
      poll(bool).ready(done)
    } else {
      self.polled = true
      poll(bool).pending
    }
  }
}

let step(remaining: ptr(mut)(i32), drops: ptr(mut)(i32), finish: bool): step = {
  step { polled: false, remaining: remaining, drops: drops, finish: finish }
}

let main(): i32 = {
  let mut remaining = 2
  let mut drops = 0
  let remaining_ptr = ptr(mut)(borrow(mut)(remaining))
  let drops_ptr = ptr(mut)(borrow(mut)(drops))
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
    { pending -> 1 }
    { ready(_) -> 0 }
  let second = match future.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let third = match future.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let fourth = match future.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let fifth = match future.poll()
    { pending -> 0 }
    { ready(value) -> value }

  let mut cancel_remaining = 2
  let mut cancel_drops = 0
  let cancel_remaining_ptr = ptr(mut)(borrow(mut)(cancel_remaining))
  let cancel_drops_ptr = ptr(mut)(borrow(mut)(cancel_drops))
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
      { pending -> () }
      { ready(_) -> () }
    match cancelled.poll()
      { pending -> () }
      { ready(_) -> () }
  }

  first + second + third + fourth + fifth + unsafe { *drops_ptr } +
    unsafe { *cancel_drops_ptr } - 2
}

test("async_await_loop_multiple.sc") {
  main() == 42
}
