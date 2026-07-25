let main(): i32 = {
  let values = [42]
  let index = 1
  values[index]
}

test("dynamic_array_oob.sc") {
  main() == 42
}
