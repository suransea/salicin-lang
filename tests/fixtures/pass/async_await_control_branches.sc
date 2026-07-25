let Poll = std.async.Poll
let Future = std.async.Future

let Step = struct {
  polled: bool,
  value: i32
}

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    if self.polled {
      Poll(i32).Ready(self.value)
    } else {
      self.polled = true
      Poll(i32).Pending
    }
  }
}

let OtherStep = struct {
  polled: bool,
  value: i32
}

extend OtherStep: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    if self.polled {
      Poll(i32).Ready(self.value)
    } else {
      self.polled = true
      Poll(i32).Pending
    }
  }
}

let step(value: i32): Step = {
  Step { polled: false, value: value }
}

let other_step(value: i32): OtherStep = {
  OtherStep { polled: false, value: value }
}

let Choice = enum {
  Left,
  Right
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
    { Pending -> () }
    { Ready(_) -> () }
  let first = match conditional.poll()
    { Ready(value) -> value }
    { Pending -> 0 }

  let mut matched = async {
    let value = match Choice.Left
      { Left -> await step(22) }
      { Right -> await other_step(0) }
    value
  }
  match matched.poll()
    { Pending -> () }
    { Ready(_) -> () }
  let second = match matched.poll()
    { Ready(value) -> value }
    { Pending -> 0 }

  first + second
}

test("async_await_control_branches.sc") {
  main() == 42
}
