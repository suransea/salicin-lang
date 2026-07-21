let Cell(T: type) = struct(value: T)

let main(): i32 = {
  let cell: Cell(bool) = Cell(42)
  42
}
