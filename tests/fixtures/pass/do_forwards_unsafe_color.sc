let read(pointer: ptr(i32)): i32 = {
  unsafe {
    do {
      return(*pointer)
      0
    }
  }
}

let main(): i32 = {
  let value = 42
  read(ptr(borrow(value)))
}

test("do_forwards_unsafe_color.sc") {
  std.test.assert(main() == 42)
}
