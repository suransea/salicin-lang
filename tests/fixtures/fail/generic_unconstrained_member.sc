let invalid(comptime t: type)(move value: t): i32 = { value.field }

let main(): i32 = { 42 }
