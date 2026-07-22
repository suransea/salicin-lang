let Number = enum {
  Value( value: i32 ),
  Empty,
}

let classify(value: Number): i32 = { value match {
  Number.Value( value: number ) if number > 40 => number,
  Number.Value( value: _ ) => 0,
  Number.Empty => 0,
}
}

let main(): i32 = { classify(Number.Value( value: 42 )) }
