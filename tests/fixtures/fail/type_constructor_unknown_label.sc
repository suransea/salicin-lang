let Cell(T: type) = struct { value: T }

let main(): i32 = {
  let value: Cell(Element: i32) = Cell(i32) { value: 0 }
  value.value
}
