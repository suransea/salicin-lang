let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T) {
  let answer(self: borrow(Self))(): i32 = { 1 }
}

extend(T: type) Cell(T) {
  let answer(self: borrow(Self))(): i32 = { 2 }
}

let main(): i32 = {
  let cell = Cell { value: 0 }
  cell.answer()
}
