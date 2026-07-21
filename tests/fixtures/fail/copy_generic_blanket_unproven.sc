let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T): Copy {}

let main(): i32 = { 42 }
