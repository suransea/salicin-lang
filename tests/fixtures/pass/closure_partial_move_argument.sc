let payload = struct { value: i32 }

let take(move payload: payload): i32 = {
  payload.value
}

let main(): i32 = {
  let base = 1
  let finish = { (move payload: payload)(tail: i32) ->
    base + take(payload) + tail
  }
  let pending = finish(payload { value: 40 })
  pending(1)
}

test("closure_partial_move_argument.sc") {
  main() == 42
}
