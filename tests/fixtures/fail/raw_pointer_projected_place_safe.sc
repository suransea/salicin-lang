let Pair = struct {
  left: i32,
  right: i32,
}

let main(): i32 = {
  let pair = Pair { left: 20, right: 22 }
  let pointer = Ptr(borrow(pair))
  (*pointer).left
}
