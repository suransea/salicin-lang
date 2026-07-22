let Value = enum { Number( value: i32 ), Empty }

let main(): i32 = { Value.Number( value: 42 ) match {
  Number( value: true ) => 1,
  Number( value: _ ) => 2,
  Empty => 0,
}
}
