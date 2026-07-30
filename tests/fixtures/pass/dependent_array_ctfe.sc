let factorial(value: usize): usize = {
  if value == 0 {
    1
  } else {
    value * factorial(value - 1)
  }
}

let first(values: array(i32)(factorial(3))): i32 = { values[0] }

let main(): i32 = { first([42, 0, 0, 0, 0, 0]) }

test("dependent_array_ctfe.sc") {
  std.test.assert(main() == 42)
}
