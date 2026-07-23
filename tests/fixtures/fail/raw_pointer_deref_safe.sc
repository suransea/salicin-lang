let main(): i32 = {
  let value = 42
  let pointer = Ptr(borrow(value))
  *pointer
}
