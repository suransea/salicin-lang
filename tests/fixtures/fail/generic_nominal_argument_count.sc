let Cell(T: type) = struct { value: T }

let main(): i32 = {
  let cell = Cell(i32, bool) { value: 42 }
  cell.value
}
