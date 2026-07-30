let payload = struct { value: i32 }

let take(move payload: payload): i32 = {
  payload.value
}

let main(): i32 = {
  let payload = payload { value: 40 }
  let add = { (x: i32)(y: i32) -> take(payload) + x + y }
  let add_one = add(1)
  add_one(1)
}

test("closure_partial_fnonce.sc") {
  std.test.assert(main() == 42)
}
