let Value = enum { Number( value: i32 ) }

let main(): i32 = { 42 match {
  Value.Number( value: value ) => value,
  _ => 0,
}
}
