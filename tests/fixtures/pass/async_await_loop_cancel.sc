let poll = core.async.poll
let future = core.async.future

let step = struct {
  polls: ptr(mut)(i32),
  drops: ptr(mut)(i32)
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
    unsafe {
      if *self.polls == 0 {
        *self.polls = 1
        poll(bool).ready(false)
      } else {
        poll(bool).pending
      }
    }
  }
}

let step(polls: ptr(mut)(i32), drops: ptr(mut)(i32)): step = {
  step { polls: polls, drops: drops }
}

let main(): i32 = {
  let mut polls = 0
  let mut drops = 0
  let polls_ptr = ptr(mut)(borrow(mut)(polls))
  let drops_ptr = ptr(mut)(borrow(mut)(drops))

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
      { pending -> 1 }
      { ready(_) -> 0 }
  }

  39 + pending + unsafe { *drops_ptr }
}

test("async_await_loop_cancel.sc") {
  main() == 42
}
