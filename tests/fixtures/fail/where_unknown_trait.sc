let identity(T: type)(value: T): T
where T: Missing = { value }

let main(): i32 = { identity(42) }
