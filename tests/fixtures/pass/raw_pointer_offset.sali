let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(i32)(8, align_of(i32))
  }
  let second = unsafe {
    raw_offset(pointer, 1)
  }
  unsafe {
    raw_init(pointer, 20)
    raw_init(second, 22)
  }
  let first_value = unsafe {
    raw_take(pointer)
  }
  let second_value = unsafe {
    raw_take(second)
  }
  unsafe {
    raw_dealloc(pointer, 8, align_of(i32))
  }
  first_value + second_value
}
