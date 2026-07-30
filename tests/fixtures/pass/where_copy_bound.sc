let marker = trait {}
let value = struct { value: i32 }
extend(value, copyable) {}
extend(value, marker()) {}

let duplicate(comptime t: type)(copy value: t): t
= requires(t is copyable && t is marker) {
  let first = value
  value
}

let main(): i32 = { duplicate(value { value: 42 }).value }

test("where_copy_bound.sc") {
  std.test.assert(main() == 42)
}
