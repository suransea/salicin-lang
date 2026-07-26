let pair = struct { left: i32, right: i32 }

extend pair: copyable {}

let consume(move pair: pair): i32 = { pair.left }

let main(): i32 = {
  let pair = pair { left: 40, right: 2 }
  consume(pair) + pair.right
}
