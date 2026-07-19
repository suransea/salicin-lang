let Choice = enum {
  Answer(i32),
  Empty,
}

extend Choice {
  let unwrap(move self)(): i32 = self match {
    Choice.Answer(value) => value,
    Choice.Empty => 0,
  }
}

let main(): i32 = {
  let choice = Choice.Answer(42)
  choice.unwrap()
}
