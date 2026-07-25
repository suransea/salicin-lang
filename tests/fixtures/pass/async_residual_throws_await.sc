let Future = std.async.Future
let Poll = std.async.Poll
let Result = std.Result
let Throws = std.error.Throws

let Step = struct {
  polls: i32,
  value: i32,
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

let make_step(fail: bool): Step with(Throws(bool)) = {
  if fail {
    throw true
  } else {
    Step { polls: 0, value: 40 }
  }
}

let run(fail: bool): i32 = {
  let result: Result(bool)(i32) = try {
    let mut future = async {
      let value = await make_step(fail)
      value + 2
    }
    let first = future.poll()
    let second = future.poll()
    match first
      { Pending -> match second
        { Ready(value) -> value }
        { Pending -> 0 } }
      { Ready(_) -> 0 }
  }

  match result
    { Ok(value) -> value }
    { Err(error) -> if error { 42 } else { 0 } }
}

let main(): i32 = {
  let success = run(false)
  let failure = run(true)
  if success == 42 && failure == 42 {
    42
  } else {
    0
  }
}

test("async_residual_throws_await.sc") {
  main() == 42
}
