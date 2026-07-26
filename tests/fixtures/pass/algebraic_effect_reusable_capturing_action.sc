let ask = effect {
  let value(): i32
}

let run(move action: (i32): i32 with(ask))(input: i32): i32 = {
  ask.handle value { (resume) -> resume(10) } action {
      action(input)
    }
}

let main(): i32 = {
  let base = 30
  let action: (i32): i32 with(ask) = { (input: i32) ->
    ask.value() + input + base
  }
  run(action)(2)
}

test("algebraic_effect_reusable_capturing_action.sc") {
  main() == 42
}
