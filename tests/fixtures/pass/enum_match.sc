let Answer = enum {
  Value( value: i32 ),
  Empty,
}

let read(value: Answer): i32 = { match value
  { Answer.Value( value: number ) -> number }
  { Answer.Empty -> 0 }
}

let main(): i32 = { read(Answer.Value( value: 42 )) }
