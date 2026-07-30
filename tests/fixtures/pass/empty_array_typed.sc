let main(): i32 = {
  let empty: array(i32)(0) = []
  42
}

test("empty_array_typed.sc") {
  std.test.assert(main() == 42)
}
