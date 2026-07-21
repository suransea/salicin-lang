let identity(T: type)(move value: T): T = { value }

let main(): i32 = { identity(i32)(40) + identity(i32)(2) }
