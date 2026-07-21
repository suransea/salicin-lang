let main(): i32 = {
  let mut anchor = 42
  let pointer = Ptr(borrow anchor)
  let reference = unsafe {
    raw_borrow(mut)(pointer, borrow(mut) anchor)
  }
  reference
}
