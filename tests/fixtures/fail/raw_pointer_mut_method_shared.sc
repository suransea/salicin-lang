let main(): i32 = {
  let value = 41
  let pointer = Ptr(borrow(value))
  unsafe {
    pointer.take()
  }
}
