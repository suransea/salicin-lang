let Number = enum {
  Value(i32),
  Empty,
}

let classify(value: Number): i32 = { value match {
  Number.Value(number) if number > 40 => number,
  Number.Value(_) => 0,
  Number.Empty => 0,
}
}

let main(): i32 = { classify(Number.Value(42)) }
