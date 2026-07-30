let main(): i32 = {
  let mut value = 40
  value = value + 2
  value
}

test("block_mutation.sc") {
  std.test.assert(main() == 42)
}
