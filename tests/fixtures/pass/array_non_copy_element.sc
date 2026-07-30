let boxed = struct { value: i32 }

let unwrap(move value: boxed): i32 = { value.value }

let main(): i32 = {
  let mut values = [boxed { value: 20 }, boxed { value: 2 }]
  let first = unwrap(values[0])
  values[0] = boxed { value: 40 }
  first + unwrap(values[0]) - unwrap(values[1]) - 16
}

test("array_non_copy_element.sc") {
  std.test.assert(main() == 42)
}
