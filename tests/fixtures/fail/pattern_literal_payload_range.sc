let value = enum { number( value: u32 ), empty }

let main(): i32 = { match value.number( value: 42 )
    { number( value: -1 ) -> 1 }
    { number( value: _ ) -> 2 }
    { empty -> 0 }
}
