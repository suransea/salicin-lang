let main(): i32 = {
  let mut values = [40, 2]
  let pointer = MutPtr(borrow(mut)(values[1]))
  let second = unsafe {
    *pointer
  }
  values[0] + second
}
