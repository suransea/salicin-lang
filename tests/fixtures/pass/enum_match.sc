let Answer = enum {
  Value(i32),
  Empty,
}

let read(value: Answer): i32 = { value match {
  Answer.Value(number) => number,
  Answer.Empty => 0,
}
}

let main(): i32 = { read(Answer.Value(42)) }
