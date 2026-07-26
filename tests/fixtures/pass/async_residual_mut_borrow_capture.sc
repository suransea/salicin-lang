let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): i32
}

let request(): i32 with(Ask) = {
  Ask.ask()
}

let poll_once(E: effect, F: type, T: type)
  (future: borrow(mut)(F)): Poll(T) with(E)
where F: Future(E, Output = T) = {
  future.poll()
}

let program(value: borrow(mut)(i32)): i32 = {
  let mut future = async {
    let amount = request()
    value = value + amount
    value
  }
  Ask.handle ask { (resume) -> resume(40) } action {
      let polled: Poll(i32) = poll_once(future)
      match polled
        { Ready(result) -> result }
        { Pending -> 0 }
    }
}

let main(): i32 = {
  let mut value = 2
  program(value)
}

test("async_residual_mut_borrow_capture.sc") {
  main() == 42
}
