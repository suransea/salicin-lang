let poll = std.async.poll
let future = std.async.future

let step = struct { polls: i32 }

extend(step, future(())) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    if self.polls == 0 {
      self.polls = 1
      poll(i32).pending
    } else {
      poll(i32).ready(41)
    }
  }
}

let main(): i32 = {
  let offset = 1
  let mut future = async {
    let value = await step { polls: 0 }
    value + offset
  }
  let first = match future.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let second = match future.poll()
    { pending -> 0 }
    { ready(value) -> value }
  first + second - 1
}

test("async_await_pending.sc") {
  main() == 42
}
