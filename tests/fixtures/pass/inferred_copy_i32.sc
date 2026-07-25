let identity(value: i32): i32 = { value }

let main(): i32 = {
  let value = 21
  identity(value) + value
}

test("inferred_copy_i32.sc") {
  main() == 42
}
