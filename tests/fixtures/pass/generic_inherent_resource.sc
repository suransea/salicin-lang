let resource = struct { counter: ptr(mut)(i32) }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let cell(comptime t: type) = struct { value: t }

extend(comptime t: type) cell(t) {
  let new(move value: t): cell(t) = { cell { value: value } }
  let take(move self)(): t = { self.value }
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *counter = 0
  }
  do {
    let cell = cell.new(resource { counter: counter })
    let resource = cell.take()
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  41 + drops
}

test("generic_inherent_resource.sc") {
  main() == 42
}
