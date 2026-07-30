let suspension = core.async.suspension
let poll = core.async.poll
let future = core.async.future
let await_source = core.async.await

let step = struct { ready: bool }

extend(step, future(())) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    if self.ready {
      ready(42)
    } else {
      self.ready = true
      pending
    }
  }
}

let main(): i32 = {
  suspension.handle
    suspend { (resume) -> resume(()) }
    action {
      await_source(step { ready: false })
    }
}

test("source_await_handler.sc") {
  std.test.assert(main() == 42)
}
