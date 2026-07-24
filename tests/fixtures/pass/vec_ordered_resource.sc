let Vec = std.vec.Vec

let Resource = struct { counter: MutPtr(i32), value: i32 }

extend Resource {
  let read(self: borrow(Self))(): i32 = { self.value }
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  }
  }}

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
    values.push(Resource { counter: counter, value: 1 })
    values.push(Resource { counter: counter, value: 3 })
    values.insert(1)(Resource { counter: counter, value: 2 })

    let mut other: Vec(Resource) = Vec(Resource).new()
    other.push(Resource { counter: counter, value: 4 })
    other.push(Resource { counter: counter, value: 5 })
    values.append(other)
    values.shrink_to_fit()

    let removed_middle = do {
      let removed = values.remove(1)
      removed.read()
    }
    let end = values.len()
    values.insert(end)(Resource { counter: counter, value: 6 })
    let last = values.len() - 1
    let removed_last = do {
      let removed = values.remove(last)
      removed.read()
    }
    let first = do {
      let removed = values.remove(0)
      removed.read()
    }
    let second = do {
      let removed = values.remove(0)
      removed.read()
    }
    let third = do {
      let removed = values.remove(0)
      removed.read()
    }
    let fourth = do {
      let removed = values.remove(0)
      removed.read()
    }
    if other.is_empty() && values.is_empty() && values.capacity() == 5 && removed_middle + removed_last + first + second + third + fourth == 21 {
      score = 36
    }
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  if drops == 6 {
    score + drops
  } else {
    0
  }
}
