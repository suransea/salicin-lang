let payload = struct { left: i32, right: i32 }

extend(payload, copyable) {}

let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(payload)(size_of(payload), align_of(payload))
  }
  unsafe {
    *pointer = payload { left: 40, right: 2 }
  }
  let value = unsafe {
    *pointer
  }
  unsafe {
    raw_dealloc(pointer, size_of(payload), align_of(payload))
  }
  value.left + value.right
}

test("raw_allocator_layout.sc") {
  main() == 42
}
