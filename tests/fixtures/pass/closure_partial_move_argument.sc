let Payload = struct { value: i32 }

let take(move payload: Payload): i32 = {
  payload.value
}

let main(): i32 = {
  let base = 1
  let finish = { (move payload: Payload)(tail: i32) ->
    base + take(payload) + tail
  }
  let pending = finish(Payload { value: 40 })
  pending(1)
}
