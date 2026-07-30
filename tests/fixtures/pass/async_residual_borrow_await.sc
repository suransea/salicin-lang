let future = core.async.future
let poll = core.async.poll

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

let make_step: with(ask)(): step = {
  step { polls: 0, value: ask.ask() }
}

let make_step_with: with(ask)(offset: borrow(i32)): step = {
  step { polls: 0, value: ask.ask() + offset }
}

let shared(offset: borrow(i32)): i32 = {
  let mut future = async {
    let value = await make_step_with(offset)
    value
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

let mutable(value: borrow(mut)(i32)): i32 = {
  let mut future = async {
    let amount = await make_step()
    value = value + amount
    value
  }
  ask.handle ask { (resume) -> resume(40) } action {
      let first = future.poll()
      let second = future.poll()
      match first
        { pending -> match second
          { ready(result) -> result }
          { pending -> 0 } }
        { ready(_) -> 0 }
    }
}

let cancelled(value: borrow(mut)(i32)): i32 = {
  do {
    let mut future = async {
      let amount = await make_step()
      value = value + amount
      value
    }
    let handled: () = ask.handle ask { (resume) -> resume(40) } action {
        match future.poll()
          { pending -> () }
          { ready(_) -> () }
      }
    handled
  }
  value = 42
  value
}

let main(): i32 = {
  let offset = 2
  let mut first = 2
  let mut second = 2
  let shared_result = shared(offset)
  let mutable_result = mutable(first)
  let cancelled_result = cancelled(second)
  if shared_result == 42 && mutable_result == 42 && cancelled_result == 42 {
    42
  } else {
    0
  }
}

test("async_residual_borrow_await.sc") {
  std.test.assert(main() == 42)
}
