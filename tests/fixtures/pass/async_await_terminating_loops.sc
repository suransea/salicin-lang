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

let step(value: i32): Step = {
  Step { polled: false, value: value }
}

let main(): i32 = {
  let mut value_loop = async {
    loop {
      break(await step(40))
    }
  }
  let loop_pending = match value_loop.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let loop_value = match value_loop.poll()
    { Pending -> 0 }
    { Ready(value) -> value }

  let mut true_while = async {
    while { true } {
      let ignored = await step(0);
      break()
    }
  }
  let while_pending = match true_while.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let while_ready = match true_while.poll()
    { Pending -> 0 }
    { Ready(_) -> 1 }

  let mut false_while = async {
    while { false } {
      let ignored = await step(0);
      break()
    }
  }
  let false_ready = match false_while.poll()
    { Pending -> 0 }
    { Ready(_) -> 1 }

  loop_value + loop_pending + while_pending + while_ready + false_ready - 2
}
