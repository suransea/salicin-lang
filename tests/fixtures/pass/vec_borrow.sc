let vec = std.vec.vec

let resource = struct { value: i32 }

extend(resource) {
  let read(self: borrow(self))(): i32 = { self.value }
}

let main(): i32 = {
  let mut values: vec(resource) = vec(resource).new()
  values.push(resource { value: 20 })
  values.push(resource { value: 0 })
  let first = do {
    let reference = values.at(0)
    reference.read()
  }
  do {
    let reference = values.at(mut)(1)
    reference.value = 21
  }
  let second = do {
    let reference = values.at(1)
    reference.read()
  }
  do {
    let reference = values.at(mut)(1)
    reference.value = 22
  }
  let third = do {
    let reference = values.at(1)
    reference.read()
  }
  if second == 21 { first + third } else { 0 }
}

test("vec_borrow.sc") {
  main() == 42
}
