let value = enum { number( value: i32 ) }

let main(): i32 = { match 42
    { value.number( value: value ) -> value }
    { _ -> 0 }
}
