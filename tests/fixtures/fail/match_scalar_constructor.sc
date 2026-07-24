let Value = enum { Number( value: i32 ) }

let main(): i32 = { match 42
  { Value.Number( value: value ) -> value }
  { _ -> 0 }
}
