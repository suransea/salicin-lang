let Cell(T: type) = struct(value: T)
let identity(T: type)(move value: T): T = { value }

let main(): i32 = { identity(Cell(i32)(42)).value }
