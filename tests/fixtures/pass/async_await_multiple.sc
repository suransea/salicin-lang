let poll = std.async.poll
let future = std.async.future

let step = struct {
  polls: i32,
  value: i32
}

extend step: future(()) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    if self.polls == 0 {
      self.polls = 1
      poll(i32).pending
    } else {
      poll(i32).ready(self.value)
    }
  }
}

let main(): i32 = {
  let mut future = async {
    let first = await step { polls: 0, value: 10 }
    let second = await step { polls: 0, value: 12 }
    let third = await step { polls: 0, value: 20 }
    first + second + third
  }

  let first_poll = match future.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let second_poll = match future.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let third_poll = match future.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let fourth_poll = match future.poll()
    { pending -> 0 }
    { ready(value) -> value }
  first_poll + second_poll + third_poll + fourth_poll - 3
}

test("async_await_multiple.sc") {
  main() == 42
}
