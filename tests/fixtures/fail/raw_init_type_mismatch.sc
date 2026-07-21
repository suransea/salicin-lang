let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(i32)(4, 4)
  }
  unsafe {
    raw_init(pointer, true)
  }
  0
}
