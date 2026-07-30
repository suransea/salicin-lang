let maybe(comptime t: type) = enum {
  some(t),
  none,
}

let unwrap(move value: maybe(i32)): i32 = { match value
    { some(item) -> item }
    { none -> 0 }
}

let main(): i32 = { unwrap(maybe(i32).some(42)) }

test("generic_enum_match.sc") {
  std.test.assert(main() == 42)
}
