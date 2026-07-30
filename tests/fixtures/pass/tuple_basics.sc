let sum(pair: (i32, i32)): i32 = { match pair
    { (left, right) -> left + right }
}

let singleton(value: (i32,)): i32 = { value.0 }

let nested(value: ((i32, i32), i32)): i32 = { match value
    { ((_, inner), tail) -> inner + tail }
}

let main(): i32 = {
  let pair: (i32, i32) = (37, 2)
  let value = ((0, 1), 1)
  sum(pair) + singleton((1,)) + nested(value)
}

test("tuple_basics.sc") {
  std.test.assert(main() == 42)
}
