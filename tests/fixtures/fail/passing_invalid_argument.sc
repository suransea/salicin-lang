let identity(M: (P: parameters): parameters, T: type)(M value: T): T = { value }

let main(): i32 = { identity(M: shared, T: i32)(42) }
