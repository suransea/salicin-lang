let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): bool
}

let Step = struct {
  drops: Ptr(mut)(i32),
  done: bool,
}

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

extend Step: Future(()) {
  let Output = bool

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(bool) = {
    Poll(bool).Ready(self.done)
  }
}

let make_step(drops: Ptr(mut)(i32)): Step with(Ask) = {
  Step { drops: drops, done: Ask.ask() }
}

let next(calls: Ptr(mut)(i32)): bool = {
  unsafe {
    *calls = *calls + 1
    *calls == 3
  }
}

let run_true(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (resume) -> resume(next(calls)) } action {
    let mut future = async {
      while { true } {
        let done = await make_step(drops)
        if done {
          break()
        } else {
          continue()
        }
      }
    }
    let first = future.poll()
    let second = future.poll()
    match first
      { Pending -> match second
        { Ready(_) -> 42 }
        { Pending -> 0 } }
      { Ready(_) -> 0 }
  }
}

let run_false(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (resume) -> resume(next(calls)) } action {
    let mut future = async {
      while { false } {
        let done = await make_step(drops)
        if done {
          break()
        } else {
          continue()
        }
      }
    }
    match future.poll()
      { Ready(_) -> 42 }
      { Pending -> 0 }
  }
}

let run_post(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (resume) -> resume(next(calls)) } action {
    let mut future = async {
      do {
        let ignored = await make_step(drops)
      }
      while { unsafe { *calls < 3 } }
    }
    let first = future.poll()
    let second = future.poll()
    match first
      { Pending -> match second
        { Ready(_) -> 42 }
        { Pending -> 0 } }
      { Ready(_) -> 0 }
  }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let true_calls = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let false_calls = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let post_calls = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
    *true_calls = 0
    *false_calls = 0
    *post_calls = 0
  }

  let true_result = run_true(drops, true_calls)
  let false_result = run_false(drops, false_calls)
  let post_result = run_post(drops, post_calls)
  let drop_count = unsafe {
    *drops
  }
  let true_count = unsafe {
    *true_calls
  }
  let false_count = unsafe {
    *false_calls
  }
  let post_count = unsafe {
    *post_calls
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
    raw_dealloc(true_calls, size_of(i32), align_of(i32))
    raw_dealloc(false_calls, size_of(i32), align_of(i32))
    raw_dealloc(post_calls, size_of(i32), align_of(i32))
  }

  if true_result == 42 && false_result == 42 && post_result == 42 &&
    drop_count == 6 && true_count == 3 &&
    false_count == 0 && post_count == 3 {
    42
  } else if true_result != 42 {
    1
  } else if false_result != 42 {
    2
  } else if post_result != 42 {
    3
  } else if drop_count != 6 {
    10 + drop_count
  } else if true_count != 3 {
    20 + true_count
  } else if false_count != 0 {
    30 + false_count
  } else {
    40 + post_count
  }
}

test("async_residual_while_await.sc") {
  main() == 42
}
