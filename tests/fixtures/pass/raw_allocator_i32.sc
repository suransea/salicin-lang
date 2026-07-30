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

test("raw_allocator_i32.sc") {
  std.test.assert(main() == 42)
}
