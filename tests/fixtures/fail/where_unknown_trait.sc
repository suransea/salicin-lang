let identity(comptime t: type)(value: t): t
= requires(t is missing) { value }

let main(): i32 = { identity(42) }
