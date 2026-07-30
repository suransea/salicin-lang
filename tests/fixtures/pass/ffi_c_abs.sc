let abs(value: i32): i32 = foreign(c)

let main(): i32 = {
  unsafe {
    abs(-42)
  }
}

test("ffi_c_abs.sc") {
  std.test.assert(main() == 42)
}
