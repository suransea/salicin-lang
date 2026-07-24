let Pair = struct { left: i32, flag: bool }

let Input = enum {
  Number( value: i32 ),
  Flag( value: bool ),
  Pair(Pair),
  Empty,
}

let classify(value: Input): i32 = { match value
  { Number( value: 40 ) -> 1 }
  { Number( value: 42 ) if true -> 20 }
  { Number( value: _ ) -> 0 }
  { Flag( value: true ) -> 10 }
  { Flag( value: false ) -> 0 }
  { Flag( value: _ ) -> 0 }
  { Pair(Pair(left: 10, flag: true)) -> 11 }
  { Pair(_) -> 0 }
  { Empty -> 0 }
}

let main(): i32 = {
  classify(Input.Number( value: 40 )) +
    classify(Input.Number( value: 42 )) +
    classify(Input.Flag( value: true )) +
    classify(Input.Pair(Pair { left: 10, flag: true }))
}
