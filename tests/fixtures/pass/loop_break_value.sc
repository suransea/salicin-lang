let main(): i32 = {
  loop {
    break(42)
  }
}

test("loop_break_value.sc") {
  std.test.assert(main() == 42)
}
