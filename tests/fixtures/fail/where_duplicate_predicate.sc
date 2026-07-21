let identity(T: type)(value: T): T
where T: Copy, T: Copy = { value }

let main(): i32 = { identity(42) }
