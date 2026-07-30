let identity(comptime t: type)(move value: t): t = { value }

let main(): i32 = {
  let value: i64 = identity(42)
  if value == 42 { 42 } else { 0 }
}

test("infer_function_from_expected.sc") {
  std.test.assert(main() == 42)
}
