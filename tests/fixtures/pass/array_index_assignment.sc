let main(): i32 = {
  let mut values = [0]
  values[0] = 42
  values[0]
}

test("array_index_assignment.sc") {
  std.test.assert(main() == 42)
}
