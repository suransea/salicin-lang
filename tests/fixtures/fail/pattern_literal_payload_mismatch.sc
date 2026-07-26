let value = enum { number( value: i32 ), empty }

let main(): i32 = { match value.number( value: 42 )
    { number( value: true ) -> 1 }
    { number( value: _ ) -> 2 }
    { empty -> 0 }
}
