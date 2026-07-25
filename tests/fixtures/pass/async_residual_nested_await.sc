let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): i32
}

let Step = struct {
  drops: Ptr(mut)(i32),
  polls: i32,
  value: i32,
}

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
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

let make_step(drops: Ptr(mut)(i32)): Step with(Ask) = {
  Step { drops: drops, polls: 0, value: Ask.ask() }
}

let run_success(drops: Ptr(mut)(i32)): i32 = {
  let mut future = async {
    let first = await make_step(drops)
    let second = await Step { drops: drops, polls: 0, value: first + 1 }
    second + 1
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

let run_cancelled(drops: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (resume) -> resume(40) } action {
    let mut future = async {
      let first = await make_step(drops)
      let second = await Step { drops: drops, polls: 0, value: first + 1 }
      second + 1
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
