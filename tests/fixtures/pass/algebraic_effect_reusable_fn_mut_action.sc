let ask = effect {
  let value(): i32
}

let run(move action: (i32): i32 with(ask))(input: i32): i32 = {
  ask.handle value { (resume) -> resume(10) } action {
      action(input)
    }
}

let main(): i32 = {
  let mut total = 0
  let mut action: (i32): i32 with(ask) = { (input: i32) ->
    total = total + input
    ask.value() + total
  }
  let mut alias = action
  let padding = 30
  let result = 1 + run(alias)(1) - 1
  result + total + padding
}

test("algebraic_effect_reusable_fn_mut_action.sc") {
  main() == 42
}
