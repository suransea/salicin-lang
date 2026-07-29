let marker = trait {}

let identity(comptime t: type)(value: t): t
= requires(t is marker && t.item == t) { value }

let main(): i32 = { identity(42) }
