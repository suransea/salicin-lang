let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(i32)(4, 3)
  }
  unsafe {
    raw_dealloc(pointer, 4, 3)
  }
  0
}
