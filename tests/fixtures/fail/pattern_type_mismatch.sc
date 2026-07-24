let Number = enum {
  Value( value: i32 ),
  Empty,
}

let classify(value: Number): i32 = { match value
  { Number.Value( value: true ) -> 42 }
  { Number.Value( value: _ ) -> 0 }
  { Number.Empty -> 0 }
}

let main(): i32 = { classify(Number.Value( value: 42 )) }
