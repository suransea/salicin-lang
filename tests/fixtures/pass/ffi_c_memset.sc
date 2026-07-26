let c_memset(
  destination: ptr(mut)(u8),
  value: i32,
  count: usize,
): ptr(mut)(u8) = foreign(c, "memset")

let main(): i32 = {
  let mut byte: u8 = 0
  do {
    let pointer = ptr(mut)(borrow(mut)(byte))
    unsafe {
      c_memset(pointer, 42, 1)
    }
  }
  if byte == 42 { 42 } else { 0 }
}

test("ffi_c_memset.sc") {
  main() == 42
}
