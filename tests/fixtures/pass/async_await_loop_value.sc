let poll = std.async.poll
let future = std.async.future

let step = struct {
  remaining: ptr(mut)(i32)
}

extend step: future(()) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    let value = unsafe {
      *self.remaining = *self.remaining - 1
      *self.remaining
    }
    poll(i32).ready(value)
  }
}

let step(remaining: ptr(mut)(i32)): step = {
  step { remaining: remaining }
}

let main(): i32 = {
  let mut remaining = 3
  let remaining_ptr = ptr(mut)(borrow(mut)(remaining))
  let mut future = async {
    loop {
      let value = await step(remaining_ptr)
      if value == 0 {
        break(value + 42)
      } else {
        continue()
      }
    }
  }

  match future.poll()
    { pending -> 0 }
    { ready(value) -> value }
}

test("async_await_loop_value.sc") {
  main() == 42
}
