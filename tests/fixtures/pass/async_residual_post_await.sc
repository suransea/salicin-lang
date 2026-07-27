let future = std.async.future
let poll = std.async.poll

let ask = effect {
  let ask(): i32
}

let step = struct {
  polls: i32,
  value: i32,
}

extend(step, future(())) {
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

let make_step(): step with(ask) = {
  step { polls: 0, value: ask.ask() }
}

let main(): i32 = {
  let mut future = async {
    let value = await make_step()
    value + 2
  }
  ask.handle ask { (resume) -> resume(40) } action {
      let first = future.poll()
      let second = future.poll()
      match first
        { pending -> match second
          { ready(value) -> value }
          { pending -> 0 } }
        { ready(_) -> 0 }
    }
}

test("async_residual_post_await.sc") {
  main() == 42
}
