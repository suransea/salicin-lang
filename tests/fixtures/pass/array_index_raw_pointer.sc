let main(): i32 = {
  let mut values = [40, 2]
  let pointer = ptr(mut)(borrow(mut)(values[1]))
  let second = unsafe {
    *pointer
  }
  values[0] + second
}

test("array_index_raw_pointer.sc") {
  std.test.assert(main() == 42)
}
