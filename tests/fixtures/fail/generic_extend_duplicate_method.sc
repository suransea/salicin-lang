let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T) {
  let answer(borrow self)(): i32 = { 1 }
}

extend(T: type) Cell(T) {
  let answer(borrow self)(): i32 = { 2 }
}

let main(): i32 = {
  let cell = Cell(0)
  cell.answer()
}
