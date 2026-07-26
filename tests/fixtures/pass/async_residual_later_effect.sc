let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): i32
}

let Resource = struct {
  drops: Ptr(mut)(i32),
  value: i32,
}

let Step = struct {
  drops: Ptr(mut)(i32),
  polls: i32,
  value: i32,
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 100
    }
  }
}

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 10
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

let record(calls: Ptr(mut)(i32), value: i32): i32 = {
  unsafe {
    *calls = *calls + 1
  }
  value
}

let run_success(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  let mut future = async {
    let retained = Resource { drops: drops, value: 1 }
    let value = await Step { drops: drops, polls: 0, value: 40 }
    value + retained.value + Ask.ask()
  }
  Ask.handle ask { (resume) -> resume(record(calls, 1)) } action {
      let pending = future.poll()
      let ready = future.poll()
      match pending
        { Pending -> match ready
          { Ready(value) -> value }
          { Pending -> 0 } }
        { Ready(_) -> 0 }
    }
}

let run_cancelled(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (resume) -> resume(record(calls, 1)) } action {
      let mut future = async {
        let retained = Resource { drops: drops, value: 1 }
        let value = await Step { drops: drops, polls: 0, value: 40 }
        value + retained.value + Ask.ask()
      }
      match future.poll()
        { Pending -> 42 }
        { Ready(_) -> 0 }
    }
}

let run_abandoned(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (_) -> record(calls, 42) } action {
      let mut future = async {
        let retained = Resource { drops: drops, value: 1 }
        let value = await Step { drops: drops, polls: 0, value: 40 }
        value + retained.value + Ask.ask()
      }
      let pending = future.poll()
      let ready = future.poll()
      match pending
        { Pending -> match ready
          { Ready(value) -> value }
          { Pending -> 0 } }
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
    drop_count == 330 && call_count == 2 {
    42
  } else {
    0
  }
}

test("async_residual_later_effect.sc") {
  main() == 42
}
