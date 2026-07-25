let Poll = std.async.Poll
let Future = std.async.Future

let Step = struct {
  polls: i32,
  value: i32
}

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    if self.polls == 0 {
      self.polls = 1
      Poll(i32).Pending
    } else {
      Poll(i32).Ready(self.value)
    }
  }
}

let main(): i32 = {
  let mut future = async {
    let first = await Step { polls: 0, value: 10 }
    let second = await Step { polls: 0, value: 12 }
    let third = await Step { polls: 0, value: 20 }
    first + second + third
  }

  let first_poll = match future.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let second_poll = match future.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let third_poll = match future.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let fourth_poll = match future.poll()
    { Pending -> 0 }
    { Ready(value) -> value }
  first_poll + second_poll + third_poll + fourth_poll - 3
}
