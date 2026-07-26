let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): bool
}

let Step = struct {
  drops: Ptr(mut)(i32),
  pending: bool,
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
    if self.pending {
      Poll(bool).Pending
    } else {
      Poll(bool).Ready(self.done)
    }
  }
}

let make_step(drops: Ptr(mut)(i32), pending: bool): Step with(Ask) = {
  Step { drops: drops, pending: pending, done: Ask.ask() }
}

let next(calls: Ptr(mut)(i32)): bool = {
  unsafe {
    *calls = *calls + 1
    *calls == 3
  }
}

let record_false(calls: Ptr(mut)(i32)): bool = {
  unsafe {
    *calls = *calls + 1
  }
  false
}

let continue_once(calls: Ptr(mut)(i32)): bool = {
  unsafe {
    *calls = *calls + 1
    *calls == 1
  }
}

let run_success(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (resume) -> resume(next(calls)) } action {
      let mut future = async {
        loop {
          let done = await make_step(drops, false)
          if done {
            break 42
          } else {
            continue()
          }
        }
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

let run_cancelled(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  Ask.handle ask { (resume) -> resume(record_false(calls)) } action {
      let mut future = async {
        loop {
          let done = await make_step(drops, true)
          if done {
            break 0
          } else {
            continue()
          }
        }
      }
      match future.poll()
        { Pending -> 42 }
        { Ready(_) -> 0 }
    }
}

let run_abandoned(drops: Ptr(mut)(i32), calls: Ptr(mut)(i32)): i32 = {
  Ask.handle ask {
      (resume) -> if continue_once(calls) {
        resume(false)
      } else {
        42
      }
    } action {
      let mut future = async {
        loop {
          let done = await make_step(drops, false)
          if done {
            break 0
          } else {
            continue()
          }
        }
      }
      match future.poll()
        { Pending -> 0 }
        { Ready(_) -> 0 }
    }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let success_calls = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let cancelled_calls = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let abandoned_calls = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
    *success_calls = 0
    *cancelled_calls = 0
    *abandoned_calls = 0
  }

  let success = run_success(drops, success_calls)
  let cancelled = run_cancelled(drops, cancelled_calls)
  let abandoned = run_abandoned(drops, abandoned_calls)
  let drop_count = unsafe {
    *drops
  }
  let success_count = unsafe {
    *success_calls
  }
  let cancelled_count = unsafe {
    *cancelled_calls
  }
  let abandoned_count = unsafe {
    *abandoned_calls
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
    raw_dealloc(success_calls, size_of(i32), align_of(i32))
    raw_dealloc(cancelled_calls, size_of(i32), align_of(i32))
    raw_dealloc(abandoned_calls, size_of(i32), align_of(i32))
  }

  if success == 42 && cancelled == 42 && abandoned == 42 &&
    drop_count == 5 && success_count == 3 &&
    cancelled_count == 1 && abandoned_count == 2 {
    42
  } else if success != 42 {
    1
  } else if cancelled != 42 {
    2
  } else if abandoned != 42 {
    3
  } else if drop_count != 5 {
    10 + drop_count
  } else if success_count != 3 {
    20 + success_count
  } else if cancelled_count != 1 {
    30 + cancelled_count
  } else {
    40 + abandoned_count
  }
}

test("async_residual_loop_await.sc") {
  main() == 42
}
