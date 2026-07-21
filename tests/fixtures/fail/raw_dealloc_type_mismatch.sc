let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(i32)(4, 4)
  }
  unsafe {
    raw_dealloc(i64)(pointer, 4, 4)
  }
  0
}
