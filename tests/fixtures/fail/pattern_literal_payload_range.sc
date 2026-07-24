let Value = enum { Number( value: u32 ), Empty }

let main(): i32 = { match Value.Number( value: 42 )
  { Number( value: -1 ) -> 1 }
  { Number( value: _ ) -> 2 }
  { Empty -> 0 }
}
