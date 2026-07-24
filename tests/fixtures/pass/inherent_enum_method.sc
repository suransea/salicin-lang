let Choice = enum {
  Answer( answer: i32 ),
  Empty,
}

extend Choice {
  let unwrap(move self)(): i32 = { match self
    { Choice.Answer( answer: value ) -> value }
    { Choice.Empty -> 0 }
  }
}

let main(): i32 = {
  let choice = Choice.Answer( answer: 42 )
  choice.unwrap()
}
