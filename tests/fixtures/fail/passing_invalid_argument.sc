let identity(P: passing, T: type)(P value: T): T = value

let main(): i32 = identity(P: shared, T: i32)(42)
