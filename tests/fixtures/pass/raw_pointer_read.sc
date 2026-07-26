let read(pointer: Ptr(i32)): i32 = {
  unsafe {
    *pointer
  }
}

let main(): i32 = {
  let value = 42
  let pointer = Ptr(borrow(value))
  read(pointer)
}

test("raw_pointer_read.sc") {
  main() == 42
}
