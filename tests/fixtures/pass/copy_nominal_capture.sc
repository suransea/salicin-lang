let Pair = struct(left: i32, right: i32)

extend Pair: Copy {}

extend Pair {
  let combine(self)(left: i32)(right: i32): i32 = { self.left + self.right + left + right }
}

let add(pair: Pair)(increment: i32): i32 = { pair.left + pair.right + increment }

let main(): i32 = {
  let pair = Pair(left: 10, right: 1)
  let add_pair = add(pair)
  let combine_pair = pair.combine(1)
  let read_pair = { () -> pair.left + pair.right }
  let valid = add_pair(0) == 11 &&
    combine_pair(1) == 13 &&
    read_pair() == 11 &&
    read_pair() == 11 &&
    pair.left == 10
  if valid {
    42
  } else {
    0
  }
}
