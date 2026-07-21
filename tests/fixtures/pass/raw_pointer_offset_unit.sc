let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(())(0, align_of(()))
  }
  let second = unsafe {
    raw_offset(pointer, 100)
  }
  unsafe {
    raw_init(second, ())
    raw_take(second)
    raw_dealloc(pointer, 0, align_of(()))
  }
  42
}
