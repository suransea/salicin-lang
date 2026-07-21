let Cell(T: type) = struct(value: T)

let main(): i32 = {
  let cell = Cell(Cell(_))(Cell(i32)(42))
  cell.value.value
}
