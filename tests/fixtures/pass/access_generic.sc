let inspect(A: access)(value: borrow(A)(i32)): i32 = { value }

let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T) {
  let view(A: access)(self: borrow(A)(Self))(): borrow(A)(T) = { borrow(A)(self.value) }
}

let main(): i32 = {
  let mut left = 1
  let right = 20
  let mut cell = Cell { value: 20 }
  do {
    let reference = cell.view(mut)()
    reference = 22
  }
  let after = do {
    let reference = cell.view()
    reference
  }
  after + inspect(right) + inspect(mut)(left) - 1
}
