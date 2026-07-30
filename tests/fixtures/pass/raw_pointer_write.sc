let main(): i32 = {
  let mut value = 40
  do {
    let pointer = ptr(mut)(borrow(mut)(value))
    unsafe {
      *pointer = *pointer + 2
    }
  }
  value
}

test("raw_pointer_write.sc") {
  std.test.assert(main() == 42)
}
