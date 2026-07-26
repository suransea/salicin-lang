let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): i32
}

let Step = struct {
  polls: i32,
  value: i32,
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

let make_step(): Step with(Ask) = {
  Step { polls: 0, value: Ask.ask() }
}

let main(): i32 = {
  let mut future = async {
    let value = await make_step()
    value + 2
  }
  Ask.handle ask { (resume) -> resume(40) } action {
      let first = future.poll()
      let second = future.poll()
      match first
        { Pending -> match second
          { Ready(value) -> value }
          { Pending -> 0 } }
        { Ready(_) -> 0 }
    }
}

test("async_residual_post_await.sc") {
  main() == 42
}
