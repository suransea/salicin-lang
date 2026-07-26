let payload = struct { value: i32 }
let pair = struct { left: payload, right: payload }

let consume_pair(move pair: pair): i32 = { pair.left.value + pair.right.value }

let main(): i32 = {
  let mut pair = pair { left: payload { value: 0 }, right: payload { value: 0 } }
  consume_pair(pair)
  pair.left = payload { value: 42 }
  let recovered = pair.left.value
  recovered + consume_pair(pair)
}
