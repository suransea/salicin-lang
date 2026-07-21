let inspect(A: access)(borrow(A) value: i32): i32 = { value }

let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T) {
  let view(A: access)(borrow(A) self)(): borrow(A) T = { borrow(A) self.value }
}

let main(): i32 = {
  let mut left = 1
  let right = 20
  let mut cell = Cell(20)
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
