let payload = struct {
  value: i32,
}

let take(move payload: payload): i32 = {
  payload.value
}

let main(): i32 = {
  let payload = payload { value: 42 }
  let choose: (bool): core.control.attempt(bool)(i32) = {
    true -> take(payload)
  }
  let attempted = choose(true)
  match attempted
    { hit(value) -> value }
    { miss(_) -> 0 }
}

test("pattern_partial_fnonce.sc") {
  std.test.assert(main() == 42)
}
