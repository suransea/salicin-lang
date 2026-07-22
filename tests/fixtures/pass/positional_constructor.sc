let Pair = struct { left: i32, right: i32 }
let Pair(left: i32, right: i32): Pair = { Pair { left: left, right: right } }

let main(): i32 = {
  let pair = Pair(40, 2)
  pair.left + pair.right
}
