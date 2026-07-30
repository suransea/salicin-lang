let main(): i32 = { 42 }

test("exit_42.sc") {
  std.test.assert(main() == 42)
}
