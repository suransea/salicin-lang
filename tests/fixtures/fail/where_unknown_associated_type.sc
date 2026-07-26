let marker = trait {}

let identity(comptime t: type)(value: t): t
where t: marker(item = t) = { value }

let main(): i32 = { identity(42) }
