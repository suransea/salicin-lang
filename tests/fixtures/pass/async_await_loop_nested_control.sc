let poll = core.async.poll
let future = core.async.future

let step = struct {
  remaining: ptr(mut)(i32)
}

extend(step, future(())) {
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

let step(remaining: ptr(mut)(i32)): step = {
  step { remaining: remaining }
}

let main(): i32 = {
  let mut remaining = 4
  let remaining_ptr = ptr(mut)(borrow(mut)(remaining))
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
    { pending -> 0 }
    { ready(_) -> 42 }
}

test("async_await_loop_nested_control.sc") {
  std.test.assert(main() == 42)
}
