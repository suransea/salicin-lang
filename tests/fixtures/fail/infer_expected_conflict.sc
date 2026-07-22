let Cell (T: type) = struct { value: T }

let main(): i32 = {
  let cell: Cell(bool) = Cell { value: 42 }
  42
}
