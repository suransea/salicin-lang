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
  let mut implicit_remaining = 3
  let implicit_ptr = ptr(mut)(borrow(mut)(implicit_remaining))
  let mut implicit = async {
    loop {
      let done = await step(implicit_ptr)
      if done {
        break(21)
      }
    }
  }
  let implicit_value = match implicit.poll()
    { pending -> 0 }
    { ready(value) -> value }

  let mut explicit_remaining = 3
  let mut fallthroughs = 0
  let explicit_ptr = ptr(mut)(borrow(mut)(explicit_remaining))
  let fallthroughs_ptr = ptr(mut)(borrow(mut)(fallthroughs))
  let mut explicit = async {
    loop {
      let done = await step(explicit_ptr)
      if done {
        break(21)
      } else {
        unsafe {
          *fallthroughs_ptr = *fallthroughs_ptr + 1
        }
      }
    }
  }
  let explicit_value = match explicit.poll()
    { pending -> 0 }
    { ready(value) -> value }

  implicit_value + explicit_value + unsafe { *fallthroughs_ptr } - 2
}

test("async_await_loop_fallthrough.sc") {
  main() == 42
}
