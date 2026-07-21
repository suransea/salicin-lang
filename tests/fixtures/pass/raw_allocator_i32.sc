let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(i32)(4, 4)
  }
  unsafe {
    *pointer = 42
  }
  let value = unsafe {
    *pointer
  }
  unsafe {
    raw_dealloc(pointer, 4, 4)
  }
  value
}
