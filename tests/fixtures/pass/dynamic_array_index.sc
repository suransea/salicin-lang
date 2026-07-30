let main(): i32 = {
  let values = [40, 2]
  let index: usize = 1
  values[0] + values[index]
}

test("dynamic_array_index.sc") {
  std.test.assert(main() == 42)
}
