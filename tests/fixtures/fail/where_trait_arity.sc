let marker(comptime a: type) = trait {}

let identity(comptime t: type)(value: t): t
where t: marker = { value }

let main(): i32 = { identity(42) }
