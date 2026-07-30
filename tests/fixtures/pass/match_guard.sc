let number = enum {
  value( value: i32 ),
  empty,
}

let classify(value: number): i32 = { match value
    { number.value( value: number ) if number > 40 -> number }
    { number.value( value: _ ) -> 0 }
    { number.empty -> 0 }
}

let main(): i32 = { classify(number.value( value: 42 )) }

test("match_guard.sc") {
  std.test.assert(main() == 42)
}
