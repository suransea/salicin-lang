let main(): i32 = {
  let value = 42
  let pointer = ptr(borrow(value))
  let same = unsafe {
    raw_offset(pointer, 0)
  }
  unsafe {
    *same
  }
}

test("raw_pointer_offset_shared.sc") {
  main() == 42
}
