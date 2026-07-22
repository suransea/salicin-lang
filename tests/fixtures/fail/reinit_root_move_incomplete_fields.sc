let Payload = struct { value: i32 }
let Pair = struct { left: Payload, right: Payload }

let consume_pair(move pair: Pair): i32 = { pair.left.value + pair.right.value }

let main(): i32 = {
  let mut pair = Pair { left: Payload { value: 0 }, right: Payload { value: 0 } }
  consume_pair(pair)
  pair.left = Payload { value: 42 }
  let recovered = pair.left.value
  recovered + consume_pair(pair)
}
