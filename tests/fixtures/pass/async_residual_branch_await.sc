let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): i32
}

let Step = struct {
  drops: Ptr(mut)(i32),
  polls: i32,
  value: i32,
  drop_amount: i32,
}

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + self.drop_amount
    }
  }
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

let run(drops: Ptr(mut)(i32), first: bool): i32 = {
  let mut future = async {
    if first {
      await Step { drops: drops, polls: 0, value: Ask.ask(), drop_amount: 10 }
    } else {
      await Step { drops: drops, polls: 0, value: Ask.ask(), drop_amount: 1 }
    }
  }
  Ask.handle ask { (resume) -> resume(40) } action {
    let pending = future.poll()
    let ready = future.poll()
    match pending
      { Pending -> match ready
        { Ready(value) -> value }
        { Pending -> 0 } }
      { Ready(_) -> 0 }
  }
}

let cancel_second(drops: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (resume) -> resume(40) } action {
    let mut future = async {
      if false {
        await Step { drops: drops, polls: 0, value: Ask.ask(), drop_amount: 10 }
      } else {
        await Step { drops: drops, polls: 0, value: Ask.ask(), drop_amount: 1 }
      }
    }
    match future.poll()
      { Pending -> 42 }
      { Ready(_) -> 0 }
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
