let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  raw_take(pointer)
}
