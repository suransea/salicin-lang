let marker(comptime a: type) = trait {}

let identity(comptime t: type)(value: t): t
= requires(t is marker) { value }

let main(): i32 = { identity(42) }
