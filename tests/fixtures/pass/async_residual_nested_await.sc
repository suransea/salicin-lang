let future = std.async.future
let poll = std.async.poll

let ask = effect {
  let ask(): i32
}

let step = struct {
  drops: ptr(mut)(i32),
  polls: i32,
  value: i32,
}

extend step: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
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

let make_step(drops: ptr(mut)(i32)): step with(ask) = {
  step { drops: drops, polls: 0, value: ask.ask() }
}

let run_success(drops: ptr(mut)(i32)): i32 = {
  let mut future = async {
    let first = await make_step(drops)
    let second = await step { drops: drops, polls: 0, value: first + 1 }
    second + 1
  }
  ask.handle ask { (resume) -> resume(40) } action {
      let first = future.poll()
      let second = future.poll()
      let third = future.poll()
      match first
        { pending -> match second
          { pending -> match third
            { ready(value) -> value }
            { pending -> 0 } }
          { ready(_) -> 0 } }
        { ready(_) -> 0 }
    }
}

let run_cancelled(drops: ptr(mut)(i32)): i32 = {
  ask.handle ask { (resume) -> resume(40) } action {
      let mut future = async {
        let first = await make_step(drops)
        let second = await step { drops: drops, polls: 0, value: first + 1 }
        second + 1
      }
      let first = future.poll()
      let second = future.poll()
      match first
        { pending -> match second
          { pending -> 42 }
          { ready(_) -> 0 } }
        { ready(_) -> 0 }
    }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
  }

  let success = run_success(drops)
  let cancelled = run_cancelled(drops)
  let drop_count = unsafe {
    *drops
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }

  if success == 42 && cancelled == 42 && drop_count == 4 {
    42
  } else {
    0
  }
}

test("async_residual_nested_await.sc") {
  main() == 42
}
