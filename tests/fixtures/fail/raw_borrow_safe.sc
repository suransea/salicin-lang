let main(): i32 = {
  let anchor = 42
  let pointer = ptr(borrow(anchor))
  let reference = raw_borrow(pointer, borrow(anchor))
  reference
}
