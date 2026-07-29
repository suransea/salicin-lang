let slice = core.memory.slice

let resource = struct {
  counter: ptr(mut)(i32),
  value: i32,
}

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
    let mut values: array(resource)(4) = [
      resource { counter: counter, value: 1 },
      resource { counter: counter, value: 2 },
      resource { counter: counter, value: 3 },
      resource { counter: counter, value: 4 },
    ]
    values.swap(0, 3)
    values.swap(1, 1)
    do {
      let view: borrow(mut)(slice(resource)) = borrow(mut)(values)
      view.reverse()
    }
    let no_drops = unsafe {
      *counter == 0
    }
    let first = do {
      let value = values.at(0)
      value.read()
    }
    let second = do {
      let value = values.at(1)
      value.read()
    }
    let third = do {
      let value = values.at(2)
      value.read()
    }
    let fourth = do {
      let value = values.at(3)
      value.read()
    }
    if no_drops && first == 1 && second == 3 && third == 2 && fourth == 4 {
      score = 38
    }
  }

  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  if drops == 4 { score + drops } else { 0 }
}

test("array_slice_reorder_resource.sc") {
  main() == 42
}
