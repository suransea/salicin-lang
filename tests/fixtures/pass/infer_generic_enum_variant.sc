let maybe(comptime t: type) = enum {
  some(t),
  none,
}

let main(): i32 = {
  let some = maybe.some(42)
  let none: maybe(i32) = maybe.none
  let from_some = match some
    { some(value) -> value }
    { none -> 0 }
  let from_none = match none
    { some(value) -> value }
    { none -> 0 }
  from_some + from_none
}

test("infer_generic_enum_variant.sc") {
  std.test.assert(main() == 42)
}
