let Poll = std.async.Poll
let Future = std.async.Future

let Step = struct {
  polls: Ptr(mut)(i32),
  drops: Ptr(mut)(i32)
}

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

extend Step: Future(()) {
  let Output = bool

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(bool) = {
    unsafe {
      if *self.polls == 0 {
        *self.polls = 1
        Poll(bool).Ready(false)
      } else {
        Poll(bool).Pending
      }
    }
  }
}

let step(polls: Ptr(mut)(i32), drops: Ptr(mut)(i32)): Step = {
  Step { polls: polls, drops: drops }
}

let main(): i32 = {
  let mut polls = 0
  let mut drops = 0
  let polls_ptr = Ptr(mut)(borrow(mut)(polls))
  let drops_ptr = Ptr(mut)(borrow(mut)(drops))

  let pending = do {
    let mut future = async {
      loop {
        let done = await step(polls_ptr, drops_ptr)
        if done {
          break()
        } else {
          continue()
        }
      }
    }
    match future.poll()
      { Pending -> 1 }
      { Ready(_) -> 0 }
  }

  39 + pending + unsafe { *drops_ptr }
}

test("async_await_loop_cancel.sc") {
  main() == 42
}
