let main(): i32 = {
  let mut value = 40
  do {
    let pointer = Ptr(mut)(borrow(mut)(value))
    unsafe {
      *pointer = *pointer + 2
    }
  }
  value
}

test("raw_pointer_write.sc") {
  main() == 42
}
