let Async = std.async.Async
let Poll = std.async.Poll
let Future = std.async.Future
let await_source = std.async.await

let Step = struct { ready: bool }

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    if self.ready {
      Ready(42)
    } else {
      self.ready = true
      Pending
    }
  }
}

let main(): i32 = {
  Async.handle
    suspend { (resume) -> resume(()) }
    action {
      await_source(Step { ready: false })
    }
}

test("source_await_handler.sc") {
  main() == 42
}
