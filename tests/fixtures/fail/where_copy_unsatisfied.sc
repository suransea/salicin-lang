let resource = struct { value: i32 }

let duplicate(comptime t: type)(copy value: t): t
= requires(t is copyable) {
  let first = value
  value
}

let main(): i32 = { duplicate(resource { value: 42 }).value }
