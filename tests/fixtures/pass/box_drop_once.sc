let box = alloc.boxed.box

let resource = struct { counter: ptr(mut)(i32) }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let main(): i32 = {
  let mut count = 0
  do {
    let counter = ptr(mut)(borrow(mut)(count))
    let first = box.new(t: resource)(resource { counter: counter })
    let second = first
  }
  41 + count
}

test("box_drop_once.sc") {
  std.test.assert(main() == 42)
}
