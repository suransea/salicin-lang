let ask = effect {
  let choose(): bool
  let value(): i32
}

let main(): i32 = {
  ask.handle choose { (resume) -> resume(false) } value { (resume) -> resume(10) } action {
      let mut left_total = 0
      let mut middle_total = 10
      let mut right_total = 20
      let mut left: with(ask)((i32): i32)  = { (value: i32) ->
        left_total = left_total + value
        ask.value() + left_total
      }
      let mut middle: with(ask)((i32): i32)  = { (value: i32) ->
        middle_total = middle_total + value
        ask.value() + middle_total
      }
      let mut right: with(ask)((i32): i32)  = { (value: i32) ->
        right_total = right_total + value
        ask.value() + right_total
      }
      let first: with(ask)((i32): i32)  = if true { left } else { middle }
      let second: with(ask)((i32): i32)  = if false { middle } else { right }
      let mut action: with(ask)((i32): i32)  = if ask.choose() { first } else { second }
      let first_result = action(1)
      let second_result = action(2)
      first_result + second_result - 22
    }
}

test("algebraic_effect_dynamic_union_fn_mut.sc") {
  std.test.assert(main() == 42)
}
