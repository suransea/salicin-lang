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
= requires(f is future(e) && f.output == t) {
  future.poll()
}

let program(value: borrow(mut)(i32)): i32 = {
  let mut future = async {
    let amount = request()
    value = value + amount
    value
  }
  ask.handle ask { (resume) -> resume(40) } action {
      let polled: poll(i32) = poll_once(future)
      match polled
        { ready(result) -> result }
        { pending -> 0 }
    }
}

let main(): i32 = {
  let mut value = 2
  program(value)
}

test("async_residual_mut_borrow_capture.sc") {
  main() == 42
}
