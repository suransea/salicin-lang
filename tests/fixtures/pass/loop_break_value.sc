let main(): i32 = { loop {
  break(42)
}
}

test("loop_break_value.sc") {
  main() == 42
}
