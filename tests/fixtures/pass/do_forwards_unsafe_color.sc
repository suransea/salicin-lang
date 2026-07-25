let read(pointer: Ptr(i32)): i32 = { unsafe {
  do {
    return(*pointer)
    0
  }
}
}

let main(): i32 = {
  let value = 42
  read(Ptr(borrow(value)))
}

test("do_forwards_unsafe_color.sc") {
  main() == 42
}
