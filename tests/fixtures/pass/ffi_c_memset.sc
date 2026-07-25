extern "C" {
  @link_name("memset")
  let c_memset(destination: Ptr(mut)(u8), value: i32, count: usize): Ptr(mut)(u8)
}

let main(): i32 = {
  let mut byte: u8 = 0
  do {
    let pointer = Ptr(mut)(borrow(mut)(byte))
    unsafe {
      c_memset(pointer, 42, 1)
    }
  }
  if byte == 42 { 42 } else { 0 }
}

test("ffi_c_memset.sc") {
  main() == 42
}
