let identity(comptime m: (comptime p: parameters): parameters, comptime t: type)(m value: t): t = { value }

let main(): i32 = { identity(m: shared, t: i32)(42) }
