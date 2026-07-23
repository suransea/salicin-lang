let shared(R: region)
  (anchor: borrow(R)(i32))(pointer: MutPtr(i32)): borrow(R)(i32) = { unsafe {
  raw_borrow(pointer, borrow(anchor))
}
  }

let mutable(R: region)
  (anchor: borrow(mut, R)(i32))(pointer: MutPtr(i32)): borrow(mut, R)(i32) = { unsafe {
  raw_borrow(mut)(pointer, borrow(mut)(anchor))
}
  }

let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    raw_init(pointer, 20)
  }
  let mut anchor = 0
  let first = do {
    let reference = shared(anchor)(pointer)
    reference
  }
  do {
    let reference = mutable(anchor)(pointer)
    reference = 22
  }
  let second = unsafe {
    raw_take(pointer)
  }
  unsafe {
    raw_dealloc(pointer, size_of(i32), align_of(i32))
  }
  first + second
}
