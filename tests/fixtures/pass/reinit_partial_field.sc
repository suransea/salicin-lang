let Payload = struct { value: i32 }
let Pair = struct { left: Payload, right: i32 }

let consume_payload(move payload: Payload): i32 = { payload.value }
let consume_pair(move pair: Pair): i32 = { pair.left.value }

let main(): i32 = {
  let mut pair = Pair { left: Payload { value: 10 }, right: 11 }
  let first = consume_payload(pair.left)
  let sibling = pair.right
  pair.left = Payload { value: 21 }
  first + sibling + consume_pair(pair)
}
