let identity(comptime t: type)(value: t): t
where t: copyable, comptime t: copyable = { value }

let main(): i32 = { identity(42) }
