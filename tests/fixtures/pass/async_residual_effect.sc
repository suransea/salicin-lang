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

let main(): i32 = {
  let mut future = async {
    request()
  }
  Ask.handle ask { (resume) -> resume(42) } action {
    let polled: Poll(i32) = poll_once(future)
    match polled
      { Ready(value) -> value }
      { Pending -> 0 }
  }
}
