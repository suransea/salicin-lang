let identity(comptime t: type)(value: t): t
where t: missing = { value }

let main(): i32 = { identity(42) }
