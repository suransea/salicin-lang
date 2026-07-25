let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): bool
}

let Step = struct {
  done: bool,
}

extend Step: Future(()) {
  let Output = bool

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(bool) = {
    Poll(bool).Ready(self.done)
  }
}

let make_step(): Step with(Ask) = {
  Step { done: Ask.ask() }
}

let main(): i32 = {
  let future = async {
    loop {
      let done = await make_step()
      if done {
        break 42
      } else {
        continue()
      }
    }
  }
  0
}
