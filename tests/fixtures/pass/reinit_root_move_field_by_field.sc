let payload = struct { value: i32 }
let pair = struct { left: payload, right: payload }

let inspect(pair: borrow(pair)): i32 = { pair.right.value }
let consume_pair(move pair: pair): i32 = { pair.left.value + pair.right.value }

let main(): i32 = {
  let mut pair = pair { left: payload { value: 0 }, right: payload { value: 0 } }
  consume_pair(pair)
  pair.left = payload { value: 10 }
  let recovered_left = pair.left.value
  pair.right = payload { value: 11 }
  recovered_left + inspect(pair) + consume_pair(pair)
}

test("reinit_root_move_field_by_field.sc") {
  main() == 42
}
