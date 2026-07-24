let main(): i32 = {
  let mut value = 42
  let pointer = Ptr(mut)(borrow(value))
  0
}
