let pair = struct {
  left: i32,
  right: i32,
}

let main(): i32 = {
  let pair = pair { left: 20, right: 22 }
  let pointer = ptr(borrow(pair))
  unsafe {
    (*pointer).left = 42
  }
  42
}
