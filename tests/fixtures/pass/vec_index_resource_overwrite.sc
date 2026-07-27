let vec = alloc.vec.vec

let resource = struct { counter: ptr(mut)(i32), value: i32 }

extend(resource) {
  let read(self: borrow(self))(): i32 = { self.value }
}

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
  let mut score = 0
  do {
    let mut values = vec.new(t: resource)()
    values.push(resource { counter: counter, value: 1 })
    values[0] = resource { counter: counter, value: 40 }
    let current = borrow(values[0])
    score = current.read()
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  score + drops
}

test("vec_index_resource_overwrite.sc") {
  main() == 42
}
