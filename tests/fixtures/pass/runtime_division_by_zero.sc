let divide(value: i32): i32 = { 42 / value }

let main(): i32 = {
  let zero = 0
  divide(zero)
}

test("runtime_division_by_zero.sc") {
  main() == 42
}
