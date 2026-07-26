let poll = std.async.poll
let future = std.async.future

let left_step = struct {
  polled: bool,
  remaining: ptr(mut)(i32)
}

let right_step = struct {
  polled: bool,
  remaining: ptr(mut)(i32)
}

extend left_step: future(()) {
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

extend right_step: future(()) {
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

let left(remaining: ptr(mut)(i32)): left_step = {
  left_step { polled: false, remaining: remaining }
}

let right(remaining: ptr(mut)(i32)): right_step = {
  right_step { polled: false, remaining: remaining }
}

let choice = enum {
  left,
  right
}

let main(): i32 = {
  let mut remaining = 3
  let remaining_ptr = ptr(mut)(borrow(mut)(remaining))
  let mut future = async {
    loop {
      let done = if unsafe { *remaining_ptr % 2 == 0 } {
        await left(remaining_ptr)
      } else {
        await right(remaining_ptr)
      }
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
    { ready(_) -> 18 }
  let conditional = first + second + third + fourth

  let mut matched_remaining = 2
  let matched_ptr = ptr(mut)(borrow(mut)(matched_remaining))
  let mut matched = async {
    loop {
      let choice = if unsafe { *matched_ptr == 2 } { choice.left } else { choice.right }
      let done = match choice
        { left -> await left(matched_ptr) }
        { right -> await right(matched_ptr) }
      if done {
        break()
      } else {
        continue()
      }
    }
  }
  let matched_first = match matched.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let matched_second = match matched.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let matched_third = match matched.poll()
    { pending -> 0 }
    { ready(_) -> 19 }

  conditional + matched_first + matched_second + matched_third
}

test("async_await_loop_branches.sc") {
  main() == 42
}
