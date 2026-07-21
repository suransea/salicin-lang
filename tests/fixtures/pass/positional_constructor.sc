let Pair = struct(left: i32, right: i32)

let main(): i32 = {
  let pair = Pair(40, 2)
  pair.left + pair.right
}
