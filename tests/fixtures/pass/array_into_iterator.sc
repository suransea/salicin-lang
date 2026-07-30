let main(): i32 = {
  let values: array(i32)(3) = [10, 11, 21]
  let mut total = 0
  for values { value ->
    total = total + value
  }
  total
}

test("array_into_iterator.sc") {
  std.test.assert(main() == 42)
}
