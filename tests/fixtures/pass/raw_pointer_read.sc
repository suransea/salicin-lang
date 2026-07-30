let read(pointer: ptr(i32)): i32 = {
  unsafe {
    *pointer
  }
}

let main(): i32 = {
  let value = 42
  let pointer = ptr(borrow(value))
  read(pointer)
}

test("raw_pointer_read.sc") {
  std.test.assert(main() == 42)
}
