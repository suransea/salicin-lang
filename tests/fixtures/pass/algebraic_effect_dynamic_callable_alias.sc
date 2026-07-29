let ask = effect {
  let value(): i32
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(10) } action {
      let mut left_total = 0
      let mut right_total = 20
      let mut left: with(ask)((i32): i32)  = { (value: i32) ->
        left_total = left_total + value
        ask.value() + left_total
      }
      let mut right: with(ask)((i32): i32)  = { (value: i32) ->
        right_total = right_total + value
        ask.value() + right_total
      }
      let mut action: with(ask)((i32): i32)  = if true { left } else { right }
      let forwarded = action
      let first = forwarded(1)
      let second = forwarded(2)
      first + second + 18
    }
}

test("algebraic_effect_dynamic_callable_alias.sc") {
  main() == 42
}
