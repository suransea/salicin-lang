let number = struct { value: i32 }

extend number {
  let make(value: i32): number = { number { value: value } }
}

let main(): i32 = { number.make(42).value }

test("inherent_associated_function.sc") {
  main() == 42
}
