let payload = struct { value: i32 }

let take(move payload: payload): i32 = { payload.value }

let main(): i32 = {
  let payload = payload { value: 42 }
  let once = { take(payload) }
  payload.value
}
