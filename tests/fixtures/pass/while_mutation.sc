let main(): i32 = {
  let mut value = 0
  while { value < 42 } {
    value = value + 1
  }
  value
}

test("while_mutation.sc") {
  std.test.assert(main() == 42)
}
