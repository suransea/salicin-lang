let number = enum {
  value( value: i32 ),
  empty,
}

let classify(value: number): i32 = { match value
    { number.value( value: true ) -> 42 }
    { number.value( value: _ ) -> 0 }
    { number.empty -> 0 }
}

let main(): i32 = { classify(number.value( value: 42 )) }
