let counter = struct { value: i32 }

let main(): i32 = {
  let mut counter = counter { value: 40 }
  counter.value = counter.value + 2
  counter.value
}

test("struct_mutation.sc") {
  main() == 42
}
