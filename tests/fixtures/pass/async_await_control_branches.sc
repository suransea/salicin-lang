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

let Choice = enum {
  Left,
  Right
}

let main(): i32 = {
  let mut conditional = async {
    let value = if true {
      await Step { polled: false, value: 20 }
    } else {
      await Step { polled: false, value: 0 }
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
      { Left -> await Step { polled: false, value: 22 } }
      { Right -> await Step { polled: false, value: 0 } }
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
