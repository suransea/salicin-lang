let Payload = struct(value: i32)
let Pair = struct(left: Payload, right: Payload)

let inspect(borrow pair: Pair): i32 = { pair.right.value }
let consume_pair(move pair: Pair): i32 = { pair.left.value + pair.right.value }

let main(): i32 = {
  let mut pair = Pair(left: Payload(0), right: Payload(0))
  consume_pair(pair)
  pair.left = Payload(10)
  let recovered_left = pair.left.value
  pair.right = Payload(11)
  recovered_left + inspect(pair) + consume_pair(pair)
}
