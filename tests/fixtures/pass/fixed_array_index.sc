let main(): i32 = {
  let values: array(i32)(2) = [40, 2]
  values[0] + values[1]
}

test("fixed_array_index.sc") {
  main() == 42
}
