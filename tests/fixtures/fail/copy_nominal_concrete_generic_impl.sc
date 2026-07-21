let Cell(T: type) = struct(value: T)

extend Cell(i32): Copy {}

let read(copy cell: Cell(i64)): i64 = { cell.value }

let main(): i32 = { 42 }
