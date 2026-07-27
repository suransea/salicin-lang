let future = core.async.future
let poll = core.async.poll

let ask = effect {
  let ask(): i32
}

let step = struct {
  drops: ptr(mut)(i32),
  polls: i32,
  value: i32,
  drop_amount: i32,
}

extend(step, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + self.drop_amount
    }
  }
}

extend(step, future(())) {
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

let run(drops: ptr(mut)(i32), first: bool): i32 = {
  let mut future = async {
    if first {
      await step { drops: drops, polls: 0, value: ask.ask(), drop_amount: 10 }
    } else {
      await step { drops: drops, polls: 0, value: ask.ask(), drop_amount: 1 }
    }
  }
  ask.handle ask { (resume) -> resume(40) } action {
      let pending = future.poll()
      let ready = future.poll()
      match pending
        { pending -> match ready
          { ready(value) -> value }
          { pending -> 0 } }
        { ready(_) -> 0 }
    }
}

let cancel_second(drops: ptr(mut)(i32)): i32 = {
  ask.handle ask { (resume) -> resume(40) } action {
      let mut future = async {
        if false {
          await step { drops: drops, polls: 0, value: ask.ask(), drop_amount: 10 }
        } else {
          await step { drops: drops, polls: 0, value: ask.ask(), drop_amount: 1 }
        }
      }
      match future.poll()
        { pending -> 42 }
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

  let first = run(drops, true)
  let second = run(drops, false)
  let cancelled = cancel_second(drops)
  let drop_count = unsafe {
    *drops
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }

  if first == 40 && second == 40 && cancelled == 42 && drop_count == 12 {
    42
  } else {
    0
  }
}

test("async_residual_branch_await.sc") {
  main() == 42
}
