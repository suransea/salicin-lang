let pair = struct { left: i32, flag: bool }

let input = enum {
  number( value: i32 ),
  flag( value: bool ),
  pair(pair),
  empty,
}

let classify(value: input): i32 = { match value
    { number( value: 40 ) -> 1 }
    { number( value: 42 ) if true -> 20 }
    { number( value: _ ) -> 0 }
    { flag( value: true ) -> 10 }
    { flag( value: false ) -> 0 }
    { flag( value: _ ) -> 0 }
    { pair(pair(left: 10, flag: true)) -> 11 }
    { pair(_) -> 0 }
    { empty -> 0 }
}

let main(): i32 = {
  classify(input.number( value: 40 )) +
    classify(input.number( value: 42 )) +
    classify(input.flag( value: true )) +
    classify(input.pair(pair { left: 10, flag: true }))
}

test("match_literal_payload.sc") {
  main() == 42
}
