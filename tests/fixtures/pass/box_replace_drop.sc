let box = std.boxed.box

let resource = struct { counter: ptr(mut)(i32), value: i32 }

extend resource: droppable {
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
    let mut boxed = box.new(t: resource)(resource { counter: counter, value: 10 })
    do {
      let previous = boxed.replace(resource { counter: counter, value: 20 })
    }
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  40 + drops
}

test("box_replace_drop.sc") {
  main() == 42
}
