let Pair = struct { values: Array(i32)(2) }

let main(): i32 = {
  let mut pair = Pair { values: [0, 2] }
  pair.values[0] = 40
  pair.values[0] + pair.values[1]
}

test("array_nested_constant_index_place.sc") {
  main() == 42
}
