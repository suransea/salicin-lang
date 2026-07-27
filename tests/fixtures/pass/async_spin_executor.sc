let executor = core.async.executor
let future = core.async.future
let poll = core.async.poll
let spin = std.async.spin

let step = struct {
  polled: bool
}

extend(step, future(())) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    if self.polled {
      poll(i32).ready(41)
    } else {
      self.polled = true
      poll(i32).pending
    }
  }
}

let main(): i32 = {
  let mut executor = spin {}
  let pending = step { polled: false }
  let ready = async { 1 }
  let first: i32 = executor.run(pending)
  let second: i32 = executor.run(ready)
  first + second
}

test("async_spin_executor.sc") {
  main() == 42
}
