let increment(value: i32): i32 = { value + 1 }

let choose(left: i32)(right: i32): i32 = {
  if left == 40 { right } else { 0 }
}

let apply(value: i32)(move action: (i32): i32): i32 = {
  action(value)
}

let Counter = struct {
  value: i32,
}

extend Counter {
  let plus(self: borrow(Self))(amount: i32): i32 = {
    self.value + amount
  }
}

let main(): i32 = {
  let first = increment 39
  let precedence = increment 38 + 1
  let second = choose first 41
  let third = apply second { (value: i32) -> value }
  let counter = Counter { value: third }
  if precedence == 40 { counter.plus 1 } else { 0 }
}

test("parenthesis_free_unary_calls.sc") {
  main() == 42
}
