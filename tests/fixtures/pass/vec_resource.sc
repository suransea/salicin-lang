use alloc.vec.Vec

let Resource = struct(counter: MutPtr(i32), value: i32)

extend Resource {
  let read(borrow self)(): i32 = { self.value }
}

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
  let mut score = 0
  do {
    let mut values: Vec(Resource) = Vec(Resource).new()
    let started_empty = values.is_empty()
    values.reserve(4)
    values.push(Resource(counter, 1))
    values.push(Resource(counter, 2))
    values.push(Resource(counter, 3))
    values.push(Resource(counter, 4))
    values.reserve(8)
    let before_remove = unsafe {
      *counter
    }
    let removed_value = do {
      let removed = values.swap_remove(1)
      removed.read()
    }
    values.truncate(2)
    values.truncate(9)
    let after_truncate = unsafe {
      *counter
    }
    values.clear()
    values.clear()
    let after_clear = unsafe {
      *counter
    }
    let ended_empty = values.is_empty()
    values.push(Resource(counter, 5))
    if started_empty && ended_empty && before_remove == 0 && removed_value == 2 && after_truncate == 2 && after_clear == 4 {
      score = 37
    }
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  if drops == 5 {
    score + drops
  } else {
    0
  }
}
