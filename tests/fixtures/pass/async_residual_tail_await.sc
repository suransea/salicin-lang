let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): i32
}

let Resource = struct {
  drops: Ptr(mut)(i32),
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let Step = struct {
  polls: i32,
  value: i32,
  resource: Resource,
}

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    if self.polls == 0 {
      self.polls = 1
      Poll(i32).Pending
    } else {
      Poll(i32).Ready(self.value)
    }
  }
}

let request(): i32 with(Ask) = {
  Ask.ask()
}

let make_step(drops: Ptr(mut)(i32)): Step with(Ask) = {
  Step { polls: 0, value: request(), resource: Resource { drops: drops } }
}

let run(drops: Ptr(mut)(i32)): i32 = {
  Ask.handle
    ask { (resume) -> resume(40) }
    action {
      let mut future = async {
        await make_step(drops)
      }
      let first = future.poll()
      let second = future.poll()
      match first
        { Pending -> match second
          { Ready(value) -> value }
          { Pending -> 0 } }
        { Ready(_) -> 0 }
    }
}

let cancel(drops: Ptr(mut)(i32)): () = {
  Ask.handle
    ask { (resume) -> resume(2) }
    action {
      let mut cancelled = async {
        await make_step(drops)
      }
      let pending = cancelled.poll()
      match pending
        { Pending -> () }
        { Ready(_) -> () }
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
