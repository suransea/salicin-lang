let answer = enum {
  value( value: i32 ),
  empty,
}

let read(value: answer): i32 = { match value
    { answer.value( value: number ) -> number }
    { answer.empty -> 0 }
}

let main(): i32 = { read(answer.value( value: 42 )) }

test("enum_match.sc") {
  std.test.assert(main() == 42)
}
