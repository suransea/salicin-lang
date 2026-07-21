let identity(T: type)(move value: T): T = value

let main(): i32 = identity(42)
