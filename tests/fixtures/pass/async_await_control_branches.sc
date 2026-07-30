let poll = core.async.poll
let future = core.async.future

let step = struct {
  polled: bool,
  value: i32
}

extend(step, future(())) {
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

let other_step = struct {
  polled: bool,
  value: i32
}

extend(other_step, future(())) {
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

let other_step(value: i32): other_step = {
  other_step { polled: false, value: value }
}

let choice = enum {
  left,
  right
}

let main(): i32 = {
  let mut conditional = async {
    let value = if true {
      let prefix = 19
      let child = await step(1)
      prefix + child
    } else {
      0
    }
    value
  }
  match conditional.poll()
    { pending -> () }
    { ready(_) -> () }
  let first = match conditional.poll()
    { ready(value) -> value }
    { pending -> 0 }

  let mut matched = async {
    let value = match choice.left
      { choice.left -> await step(22) }
      { choice.right -> await other_step(0) }
    value
  }
  match matched.poll()
    { pending -> () }
    { ready(_) -> () }
  let second = match matched.poll()
    { ready(value) -> value }
    { pending -> 0 }

  first + second
}

test("async_await_control_branches.sc") {
  std.test.assert(main() == 42)
}
