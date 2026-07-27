let future = std.async.future
let poll = std.async.poll

let ask = effect {
  let ask(): bool
}

let step = struct {
  done: bool,
}

extend(step, future(())) {
  let output = bool

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(bool) = {
    poll(bool).ready(self.done)
  }
}

let make_step(): step with(ask) = {
  step { done: ask.ask() }
}

let main(): i32 = {
  ask.handle ask { (resume) -> resume(false) } action {
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
        { ready(value) -> value }
        { pending -> 0 }
    }
}
