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
  Ask.handle ask { (resume) -> resume(false) } action {
    let mut future = async {
      loop {
        let first = await make_step()
        let second = await make_step()
        if second {
          break 42
        } else {
          continue()
        }
      }
    }
    match future.poll()
      { Ready(value) -> value }
      { Pending -> 0 }
  }
}
