let decide = effect {
  let choose(): bool
}

let choose_value(): bool with(decide) = {
  decide.choose()
}

let main(): i32 = {
  decide.handle choose { (resume) -> resume(true) } action {
      if choose_value() { 42 } else { 0 }
    }
}

test("algebraic_effect_cross_function_answer.sc") {
  main() == 42
}
