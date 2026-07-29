let slice = core.memory.slice

let main(): i32 = {
  let mut values: array(i32)(2) = [40, 1]
  do {
    let view: borrow(mut)(slice(i32)) = borrow(mut)(values)
    let pointer = unsafe {
      raw_slice_ptr(mut)(view)
    }
    unsafe {
      *raw_offset(pointer, 1) = 2
    }
  }
  let view: borrow(slice(i32)) = borrow(values)
  let pointer = unsafe {
    raw_slice_ptr(view)
  }
  unsafe {
    *pointer + *raw_offset(pointer, 1)
  }
}

test("raw_slice_pointer.sc") {
  main() == 42
}
