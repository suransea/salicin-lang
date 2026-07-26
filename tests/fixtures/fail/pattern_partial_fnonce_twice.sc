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
  let first = choose(true)
  let second = choose(false)
  42
}
