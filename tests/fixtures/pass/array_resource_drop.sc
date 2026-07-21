let Resource = struct(counter: MutPtr(i32))

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = { unsafe {
    *self.counter = *self.counter + 1
  }
  }
}

let consume(move value: Resource): () = { () }

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *counter = 0
  }
  do {
    let mut values = [Resource(counter), Resource(counter)]
    consume(values[0])
    values[0] = Resource(counter)
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  39 + drops
}
