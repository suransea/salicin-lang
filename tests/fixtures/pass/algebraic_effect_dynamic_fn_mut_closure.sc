let ask = effect {
  let value(): i32
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(10) } action {
      let mut left_total = 0
      let mut right_total = 20
      let mut left: (i32): i32 with(ask) = { (value: i32) ->
        left_total = left_total + value
        ask.value() + left_total
      }
      let mut right: (i32): i32 with(ask) = { (value: i32) ->
        right_total = right_total + value
        ask.value() + right_total
      }
      let mut action: (i32): i32 with(ask) = if true { left } else { right }
      let first = action(1)
      let second = action(2)
      first + second + 18
    }
}

test("algebraic_effect_dynamic_fn_mut_closure.sc") {
  main() == 42
}
