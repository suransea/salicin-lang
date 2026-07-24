let Payload = struct { value: i32 }

let take(move payload: Payload): i32 = {
  payload.value
}

let main(): i32 = {
  let payload = Payload { value: 40 }
  let add = { (x: i32)(y: i32) -> take(payload) + x + y }
  let add_one = add(1)
  add_one(1)
}
