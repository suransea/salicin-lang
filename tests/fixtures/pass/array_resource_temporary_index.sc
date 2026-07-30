let resource = struct { counter: ptr(mut)(i32) }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let make(counter: ptr(mut)(i32)): array(resource)(2) = { [resource { counter: counter }, resource { counter: counter }] }

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *counter = 0
  }
  do {
    let first = [resource { counter: counter }, resource { counter: counter }][0]
    let second = make(counter)[1]
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  38 + drops
}

test("array_resource_temporary_index.sc") {
  std.test.assert(main() == 42)
}
