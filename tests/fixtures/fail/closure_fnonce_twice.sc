let Payload = struct { value: i32 }

let take(move payload: Payload): i32 = { payload.value }

let main(): i32 = {
  let payload = Payload { value: 42 }
  let once = { take(payload) }
  let first = once()
  once()
}
