let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let mut anchor = 0
  let reference = unsafe {
    raw_borrow(pointer, borrow(anchor))
  }
  anchor = 1
  reference
}
