let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): i32
}

let Resource = struct {
  value: i32,
  drops: Ptr(mut)(i32)
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let request(): i32 with(Ask) = {
  Ask.ask()
}

let consume(move resource: Resource): i32 = {
  resource.value
}

let poll_once(E: effect, F: type, T: type)
  (future: borrow(mut)(F)): Poll(T) with(E)
where F: Future(E, Output = T) = {
  future.poll()
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
  }
  let resource = Resource { value: 2, drops: drops }
  let mut future = async {
    consume(resource) + request()
  }
  let result: i32 = Ask.handle ask { (resume) -> resume(40) } action {
    let polled: Poll(i32) = poll_once(future)
    match polled
      { Ready(value) -> value }
      { Pending -> 0 }
  }
  let drop_count = unsafe {
    *drops
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  if result == 42 && drop_count == 1 { 42 } else { 0 }
}

test("async_residual_move_capture.sc") {
  main() == 42
}
