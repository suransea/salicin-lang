let Pair = struct(left: i32, right: i32)

extend Pair: Copy {}

let consume(move pair: Pair): i32 = { pair.left }

let main(): i32 = {
  let pair = Pair(left: 40, right: 2)
  consume(pair) + pair.right
}
