let resource = struct { counter: ptr(mut)(i32) }
let batch = struct { values: array(resource)(2) }

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
  do {
    let batch = batch { values: [resource { counter: counter }, resource { counter: counter }] }
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  40 + drops
}

test("array_resource_nested_drop.sc") {
  std.test.assert(main() == 42)
}
