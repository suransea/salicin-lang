let main(): i32 = {
  let values = [42]
  let index: usize = 1
  values[index]
}

test("dynamic_array_oob.sc") {
  std.test.assert(main() == 42)
}
