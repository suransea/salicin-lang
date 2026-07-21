let Pair = struct(left: i32, right: i32)

let increment_both(borrow(mut) left: i32, borrow(mut) right: i32): () = {
  left = left + 1
  right = right + 1
}

let main(): i32 = {
  let mut pair = Pair(left: 21, right: 21)
  increment_both(pair.left, pair.left)
  pair.left
}
