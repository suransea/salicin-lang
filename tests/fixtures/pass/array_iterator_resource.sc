let resource = struct {
  value: i32,
  drops: ptr(mut)(i32),
}

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let read(value: borrow(resource)): i32 = { value.value }

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let total = do {
    let values: array(resource)(3) = [
      resource { value: 9, drops: drops },
      resource { value: 12, drops: drops },
      resource { value: 21, drops: drops },
    ]
    let mut sum = 0
    for values.iter() { value ->
      sum = sum + read(value)
    }
    sum
  }
  let drop_count = unsafe { *drops }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  total + drop_count - 3
}

test("array_iterator_resource.sc") {
  main() == 42
}
