let Poll = std.async.Poll
let Future = std.async.Future

let Step = struct { polls: i32 }

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    if self.polls == 0 {
      self.polls = 1
      Poll(i32).Pending
    } else {
      Poll(i32).Ready(41)
    }
  }
}

let main(): i32 = {
  let offset = 1
  let mut future = async {
    let value = await Step { polls: 0 }
    value + offset
  }
  let first = match future.poll()
    { Pending -> 1 }
    { Ready(_) -> 0 }
  let second = match future.poll()
    { Pending -> 0 }
    { Ready(value) -> value }
  first + second - 1
}
