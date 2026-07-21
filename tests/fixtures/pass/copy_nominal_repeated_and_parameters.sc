let Pair = struct(left: i32, right: i32)

extend Pair: Copy {}

let read_left(pair: Pair): i32 = { pair.left }

let read_right(copy pair: Pair): i32 = { pair.right }

let main(): i32 = {
  let pair = Pair(left: 10, right: 11)
  let first = pair
  let second = pair
  first.left + second.right + read_left(pair) + read_right(pair)
}
