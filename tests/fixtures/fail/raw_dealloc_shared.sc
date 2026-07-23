let main(): i32 = {
  let value = 42
  let pointer = Ptr(borrow(value))
  unsafe {
    raw_dealloc(pointer, 4, 4)
  }
  0
}
