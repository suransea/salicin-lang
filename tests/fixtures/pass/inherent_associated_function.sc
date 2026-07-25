let Number = struct { value: i32 }

extend Number {
  let make(value: i32): Number = { Number { value: value } }
}

let main(): i32 = { Number.make(42).value }

test("inherent_associated_function.sc") {
  main() == 42
}
