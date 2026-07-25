let Poll = std.async.Poll
let Future = std.async.Future

let LeftStep = struct {
  polled: bool,
  remaining: Ptr(mut)(i32)
}

let RightStep = struct {
  polled: bool,
  remaining: Ptr(mut)(i32)
}

extend LeftStep: Future(()) {
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

extend RightStep: Future(()) {
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

let left(remaining: Ptr(mut)(i32)): LeftStep = {
  LeftStep { polled: false, remaining: remaining }
}

let right(remaining: Ptr(mut)(i32)): RightStep = {
  RightStep { polled: false, remaining: remaining }
}

let Choice = enum {
  Left,
  Right
}

let main(): i32 = {
  let mut remaining = 3
  let remaining_ptr = Ptr(mut)(borrow(mut)(remaining))
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
    { Ready(_) -> 18 }
  let conditional = first + second + third + fourth

  let mut matched_remaining = 2
  let matched_ptr = Ptr(mut)(borrow(mut)(matched_remaining))
  let mut matched = async {
    loop {
      let choice = if unsafe { *matched_ptr == 2 } { Choice.Left } else { Choice.Right }
      let done = match choice
        { Left -> await left(matched_ptr) }
        { Right -> await right(matched_ptr) }
      if done {
        break()
      } else {
        continue()
      }
    }
  }
  let matched_first = match matched.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let matched_second = match matched.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let matched_third = match matched.poll()
    { Pending -> 0 }
    { Ready(_) -> 19 }

  conditional + matched_first + matched_second + matched_third
}

test("async_await_loop_branches.sc") {
  main() == 42
}
