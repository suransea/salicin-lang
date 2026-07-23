let Cell = struct { value: i32 }
let read(T: type)(value: borrow(T)): i32 = { 42 }

let main(): i32 = { read(Cell { value: 42 }) }
