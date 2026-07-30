let identity(comptime t: type)(move value: t): t = { value }

let main(): i32 = { identity(42) }

test("infer_generic_function.sc") {
  std.test.assert(main() == 42)
}
