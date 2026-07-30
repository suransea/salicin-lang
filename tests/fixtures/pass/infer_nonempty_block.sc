let identity(comptime t: type)(move value: t): t = { value }

let main(): i32 = {
  identity(do { let value = 42; value });
  42
}

test("infer_nonempty_block.sc") {
  std.test.assert(main() == 42)
}
