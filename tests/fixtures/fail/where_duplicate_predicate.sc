let identity(comptime t: type)(value: t): t
= requires(t is copyable && t is copyable) { value }

let main(): i32 = { identity(42) }
