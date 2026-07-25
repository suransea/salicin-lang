let take(value: ()): i32 = { 42 }

let main(): i32 = {
  let mut value = ()
  value = ()
  take(value)
}

test("unit_values.sc") {
  main() == 42
}
