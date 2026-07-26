let future = std.async.future
let poll = std.async.poll

let ask = effect {
  let ask(): i32
}

let resource = struct {
  drops: ptr(mut)(i32),
}

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let step = struct {
  polls: i32,
  value: i32,
  resource: resource,
}

extend step: future(()) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    if self.polls == 0 {
      self.polls = 1
      poll(i32).pending
    } else {
      poll(i32).ready(self.value)
    }
  }
}

let request(): i32 with(ask) = {
  ask.ask()
}

let make_step(drops: ptr(mut)(i32)): step with(ask) = {
  step { polls: 0, value: request(), resource: resource { drops: drops } }
}

let run(drops: ptr(mut)(i32)): i32 = {
  ask.handle
    ask { (resume) -> resume(40) }
    action {
      let mut future = async {
        await make_step(drops)
      }
      let first = future.poll()
      let second = future.poll()
      match first
        { pending -> match second
          { ready(value) -> value }
          { pending -> 0 } }
        { ready(_) -> 0 }
    }
}

let cancel(drops: ptr(mut)(i32)): () = {
  ask.handle
    ask { (resume) -> resume(2) }
    action {
      let mut cancelled = async {
        await make_step(drops)
      }
      let pending = cancelled.poll()
      match pending
        { pending -> () }
        { ready(_) -> () }
    }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
  }

  let value = run(drops)
  cancel(drops)
  let drop_count = unsafe {
    *drops
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  value + drop_count
}

test("async_residual_tail_await.sc") {
  main() == 42
}
