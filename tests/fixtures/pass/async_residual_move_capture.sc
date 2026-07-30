let future = core.async.future
let poll = core.async.poll

let ask = effect {
  let ask(): i32
}

let resource = struct {
  value: i32,
  drops: ptr(mut)(i32)
}

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let request: with(ask)(): i32 = {
  ask.ask()
}

let consume(move resource: resource): i32 = {
  resource.value
}

let poll_once(comptime e: effects, comptime f: type, comptime t: type): with(e)(future: borrow(mut)(f)): poll(t) = requires(f is future(e) && f.output == t) {
  future.poll()
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
  }
  let resource = resource { value: 2, drops: drops }
  let mut future = async {
    consume(resource) + request()
  }
  let result: i32 = ask.handle ask { (resume) -> resume(40) } action {
      let polled: poll(i32) = poll_once(future)
      match polled
        { ready(value) -> value }
        { pending -> 0 }
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
  std.test.assert(main() == 42)
}
