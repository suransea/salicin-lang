let payload = struct { value: i32 }
let pair = struct { left: payload, right: i32 }

let consume_payload(move payload: payload): i32 = { payload.value }
let consume_pair(move pair: pair): i32 = { pair.left.value }

let main(): i32 = {
  let mut pair = pair { left: payload { value: 10 }, right: 11 }
  let first = consume_payload(pair.left)
  let sibling = pair.right
  pair.left = payload { value: 21 }
  first + sibling + consume_pair(pair)
}

test("reinit_partial_field.sc") {
  std.test.assert(main() == 42)
}
