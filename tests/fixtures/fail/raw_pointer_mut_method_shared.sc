let main(): i32 = {
  let value = 41
  let pointer = ptr(borrow(value))
  unsafe {
    pointer.take()
  }
}
