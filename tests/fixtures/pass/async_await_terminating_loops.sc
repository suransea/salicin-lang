let poll = std.async.poll
let future = std.async.future

let step = struct {
  polled: bool,
  value: i32
}

extend step: future(()) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    if self.polled {
      poll(i32).ready(self.value)
    } else {
      self.polled = true
      poll(i32).pending
    }
  }
}

let step(value: i32): step = {
  step { polled: false, value: value }
}

let condition = struct {
  polled: bool,
  value: bool
}

extend condition: future(()) {
  let output = bool

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(bool) = {
    if self.polled {
      poll(bool).ready(self.value)
    } else {
      self.polled = true
      poll(bool).pending
    }
  }
}

let condition(value: bool): condition = {
  condition { polled: false, value: value }
}

let main(): i32 = {
  let mut value_loop = async {
    loop {
      break(await step(40))
    }
  }
  let loop_pending = match value_loop.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let loop_value = match value_loop.poll()
    { pending -> 0 }
    { ready(value) -> value }

  let mut true_while = async {
    while { true } {
      let ignored = await step(0);
      break()
    }
  }
  let while_pending = match true_while.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let while_ready = match true_while.poll()
    { pending -> 0 }
    { ready(_) -> 1 }

  let mut false_while = async {
    while { false } {
      let ignored = await step(0);
      break()
    }
  }
  let false_ready = match false_while.poll()
    { pending -> 0 }
    { ready(_) -> 1 }

  let mut awaited_condition = async {
    while { await condition(false) } {
      break()
    }
  }
  let condition_pending = match awaited_condition.poll()
    { pending -> 1 }
    { ready(_) -> 0 }
  let condition_ready = match awaited_condition.poll()
    { pending -> 0 }
    { ready(_) -> 1 }

  loop_value + loop_pending + while_pending + while_ready + false_ready + condition_pending +
    condition_ready - 4
}

test("async_await_terminating_loops.sc") {
  main() == 42
}
