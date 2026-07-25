let Payload = struct {
  value: i32,
}

let take(move payload: Payload): i32 = {
  payload.value
}

let main(): i32 = {
  let payload = Payload { value: 42 }
  let choose: (bool): core.control.Attempt(bool)(i32) = {
    true -> take(payload)
  }
  let attempted = choose(true)
  match attempted
    { Hit(value) -> value }
    { Miss(_) -> 0 }
}

test("pattern_partial_fnonce.sc") {
  main() == 42
}
