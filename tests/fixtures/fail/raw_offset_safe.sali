let main(): i32 = {
  let pointer = unsafe { raw_alloc(i32)(4, 4) }
  let second = raw_offset(pointer, 1)
  unsafe { raw_dealloc(pointer, 4, 4) }
  42
}
