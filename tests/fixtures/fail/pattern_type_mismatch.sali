let Number = enum {
  Value(i32),
  Empty,
}

let classify(value: Number): i32 = value match {
  Number.Value(true) => 42,
  Number.Value(_) => 0,
  Number.Empty => 0,
}

let main(): i32 = classify(Number.Value(42))
