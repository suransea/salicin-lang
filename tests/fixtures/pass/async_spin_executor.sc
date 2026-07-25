let Executor = std.async.Executor
let Future = std.async.Future
let Poll = std.async.Poll
let Spin = std.async.Spin

let Step = struct {
  polled: bool
}

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    if self.polled {
      Poll(i32).Ready(41)
    } else {
      self.polled = true
      Poll(i32).Pending
    }
  }
}

let main(): i32 = {
  let mut executor = Spin {}
  let pending = Step { polled: false }
  let ready = async { 1 }
  let first: i32 = executor.run(pending)
  let second: i32 = executor.run(ready)
  first + second
}

test("async_spin_executor.sc") {
  main() == 42
}
