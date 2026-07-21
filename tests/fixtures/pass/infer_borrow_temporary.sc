let Cell = struct(value: i32)
let read(T: type)(borrow value: T): i32 = { 42 }

let main(): i32 = { read(Cell(42)) }
