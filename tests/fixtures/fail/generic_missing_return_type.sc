let identity(T: type)(move value: T) = value

let main(): i32 = identity(i32)(42)
