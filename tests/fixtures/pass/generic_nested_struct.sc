let Cell (T: type) = struct { value: T }

let main(): i32 = {
  let inner = Cell(i32) { value: 42 }
  let outer = Cell(Cell(i32)) { value: inner }
  outer.value.value
}
