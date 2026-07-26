let main(): i32 = {
  let value = 42
  let pointer = ptr(borrow(value))
  unsafe {
    *pointer = 0
  }
  0
}
