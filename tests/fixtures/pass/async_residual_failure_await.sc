let future = core.async.future
let poll = core.async.poll
let result = core.result
let throwing = core.error.throwing

let step = struct {
  polls: i32,
  value: i32,
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

let make_step: with(throwing(bool))(fail: bool): step = {
  if fail {
    throw true
  } else {
    step { polls: 0, value: 40 }
  }
}

let run(fail: bool): i32 = {
  let result: result(bool)(i32) = try {
    let mut future = async {
      let value = await make_step(fail)
      value + 2
    }
    let first = future.poll()
    let second = future.poll()
    match first
      { pending -> match second
        { ready(value) -> value }
        { pending -> 0 } }
      { ready(_) -> 0 }
  }

  match result
    { ok(value) -> value }
    { err(error) -> if error { 42 } else { 0 } }
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

test("async_residual_failure_await.sc") {
  main() == 42
}
