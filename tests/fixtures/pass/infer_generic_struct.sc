let Cell(T: type) = struct(value: T)

let main(): i32 = { Cell(42).value }
