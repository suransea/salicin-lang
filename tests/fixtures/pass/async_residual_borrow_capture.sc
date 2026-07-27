let future = core.async.future
let poll = core.async.poll

let ask = effect {
  let ask(): i32
}

let request(): i32 with(ask) = {
  ask.ask()
}

let poll_once(comptime e: effects, comptime f: type, comptime t: type)
  (future: borrow(mut)(f)): poll(t) with(e)
where f: future(e, output = t) = {
  future.poll()
}

let program(offset: borrow(i32)): i32 = {
  let mut future = async {
    request() + offset
  }
  ask.handle ask { (resume) -> resume(40) } action {
      let polled: poll(i32) = poll_once(future)
      match polled
        { ready(value) -> value }
        { pending -> 0 }
    }
}

let main(): i32 = {
  let offset = 2
  program(offset)
}

test("async_residual_borrow_capture.sc") {
  main() == 42
}
