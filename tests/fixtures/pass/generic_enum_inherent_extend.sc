let maybe(comptime t: type) = enum {
  some(t),
  none,
}

extend(maybe(t)) {
  let unwrap_or(move self)(move fallback: t): t = { match self
      { some(value) -> value }
      { none -> fallback }
  }
}

let main(): i32 = {
  let value = maybe.some(42)
  value.unwrap_or(0)
}

test("generic_enum_inherent_extend.sc") {
  std.test.assert(main() == 42)
}
