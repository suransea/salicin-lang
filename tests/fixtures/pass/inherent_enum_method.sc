let choice = enum {
  answer( answer: i32 ),
  empty,
}

extend(choice) {
  let unwrap(move self)(): i32 = { match self
      { choice.answer( answer: value ) -> value }
      { choice.empty -> 0 }
  }
}

let main(): i32 = {
  let choice = choice.answer( answer: 42 )
  choice.unwrap()
}

test("inherent_enum_method.sc") {
  main() == 42
}
