let Poll = std.async.Poll
let Future = std.async.Future

let Step = struct {
  remaining: Ptr(mut)(i32)
}

extend Step: Future(()) {
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

let step(remaining: Ptr(mut)(i32)): Step = {
  Step { remaining: remaining }
}

let main(): i32 = {
  let mut remaining = 4
  let remaining_ptr = Ptr(mut)(borrow(mut)(remaining))
  let mut future = async {
    loop {
      if unsafe { *remaining_ptr % 2 == 0 } {
        let done = await step(remaining_ptr)
        if done {
          break()
        } else {
          continue()
        }
      } else {
        let done = await step(remaining_ptr)
        if done {
          break()
        } else {
          ()
        }
      }
    }
  }

  match future.poll()
    { Pending -> 0 }
    { Ready(_) -> 42 }
}

test("async_await_loop_nested_control.sc") {
  main() == 42
}
