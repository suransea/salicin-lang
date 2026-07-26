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

let make_second(
  drops: Ptr(mut)(i32),
  calls: Ptr(mut)(i32),
  first: i32,
): Step with(Ask) = {
  unsafe {
    *calls = *calls + 1
  }
  Step { drops: drops, polls: 0, value: first + Ask.ask(), drop_amount: 1 }
}

let abandon(calls: Ptr(mut)(i32)): i32 = {
  unsafe {
    *calls = *calls + 1
  }
  42
}

let run_success(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  let mut future = async {
    let first = await Step { drops: drops, polls: 0, value: 2, drop_amount: 10 }
    let second = await make_second(drops, calls, first)
    second
  }
  Ask.handle ask { (resume) -> resume(40) } action {
      let first = future.poll()
      let second = future.poll()
      let third = future.poll()
      match first
        { Pending -> match second
          { Pending -> match third
            { Ready(value) -> value }
            { Pending -> 0 } }
          { Ready(_) -> 0 } }
        { Ready(_) -> 0 }
    }
}

let run_cancelled(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (resume) -> resume(40) } action {
      let mut future = async {
        let first = await Step { drops: drops, polls: 0, value: 2, drop_amount: 10 }
        let second = await make_second(drops, calls, first)
        second
      }
      let first = future.poll()
      let second = future.poll()
      match first
        { Pending -> match second
          { Pending -> 42 }
          { Ready(_) -> 0 } }
        { Ready(_) -> 0 }
    }
}

let run_abandoned(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (_) -> abandon(calls) } action {
      let mut future = async {
        let first = await Step { drops: drops, polls: 0, value: 2, drop_amount: 10 }
        let second = await make_second(drops, calls, first)
        second
      }
      let first = future.poll()
      let second = future.poll()
      match first
        { Pending -> match second
          { Pending -> 0 }
          { Ready(_) -> 0 } }
        { Ready(_) -> 0 }
    }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let calls = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
    *calls = 0
  }

  let success = run_success(drops, calls)
  let cancelled = run_cancelled(drops, calls)
  let abandoned = run_abandoned(drops, calls)
  let drop_count = unsafe {
    *drops
  }
  let call_count = unsafe {
    *calls
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
    raw_dealloc(calls, size_of(i32), align_of(i32))
  }

  if success == 42 && cancelled == 42 && abandoned == 42 &&
    drop_count == 32 && call_count == 4 {
    42
  } else {
    0
  }
}

test("async_residual_later_await.sc") {
  main() == 42
}
