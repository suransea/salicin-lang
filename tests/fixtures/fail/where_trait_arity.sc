let Marker(A: type) = trait {}

let identity(T: type)(value: T): T
where T: Marker = { value }

let main(): i32 = { identity(42) }
