let main(): i32 = {
  let values = [1, 2]
  let shared = ptr(borrow(values[0]))
  let pointer: ptr(mut)(i32) = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    pointer.init(40)
    let shared_result = *shared.offset(1)
    let selected = pointer.offset(0)
    let value = selected.take()
    pointer.init(value)
    let result = *pointer
    raw_dealloc(pointer, size_of(i32), align_of(i32))
    result + shared_result
  }
}

test("raw_pointer_methods.sc") {
  main() == 42
}
