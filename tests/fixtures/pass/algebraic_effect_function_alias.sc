let ask = effect {
  let value(): i32
}

let ask(): i32 with(ask) = {
  ask.value()
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
      let action = ask
      let forwarded = action
      forwarded()
    }
}

test("algebraic_effect_function_alias.sc") {
  main() == 42
}
