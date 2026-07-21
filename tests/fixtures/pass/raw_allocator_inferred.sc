let main(): i32 = {
  let pointer: MutPtr(Array(i32, 2)) = unsafe {
    raw_alloc(size: 8, align: 64)
  }
  unsafe {
    *pointer = [40, 2]
  }
  let values = unsafe {
    *pointer
  }
  unsafe {
    raw_dealloc(pointer: pointer, size: 8, align: 64)
  }
  values[0] + values[1]
}
