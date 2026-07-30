let resource = struct { counter: ptr(mut)(i32) }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *counter = 0
  }

  let mut iteration = 0
  loop {
    iteration = iteration + 1
    if iteration < 3 {
      let resource = resource { counter: counter }
      continue()
    }
    break()
  }

  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  40 + drops
}

test("continue_cleanup.sc") {
  std.test.assert(main() == 42)
}
