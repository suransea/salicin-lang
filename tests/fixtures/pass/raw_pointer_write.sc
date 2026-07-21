let main(): i32 = {
  let mut value = 40
  do {
    let pointer = MutPtr(borrow(mut) value)
    unsafe {
      *pointer = *pointer + 2
    }
  }
  value
}
