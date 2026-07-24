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
    let mut empty: Vec(Resource) = Vec(Resource).new()
    empty.reverse()

    let mut values: Vec(Resource) = Vec(Resource).new()
    values.push(Resource { counter: counter, value: 1 })
    values.push(Resource { counter: counter, value: 2 })
    values.push(Resource { counter: counter, value: 3 })
    values.push(Resource { counter: counter, value: 4 })
    values.swap(0, 3)
    values.swap(1, 1)
    values.reverse()
    let no_drops = unsafe {
      *counter == 0
    }
    let first = do {
      let reference = values.at(0)
      reference.read()
    }
    let second = do {
      let reference = values.at(1)
      reference.read()
    }
    let third = do {
      let reference = values.at(2)
      reference.read()
    }
    let fourth = do {
      let reference = values.at(3)
      reference.read()
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
