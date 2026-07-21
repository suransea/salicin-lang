use alloc.boxed.{box_new, box_replace}

let Resource = struct(counter: MutPtr(i32), value: i32)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = { unsafe {
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
    let mut boxed = box_new(Resource(counter, 10))
    do {
      let previous = box_replace(boxed)(Resource(counter, 20))
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
