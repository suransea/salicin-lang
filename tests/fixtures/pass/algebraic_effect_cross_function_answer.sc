let Decide = effect {
  let choose(): bool
}

let choose_value(): bool with(Decide) = {
  Decide.choose()
}

let main(): i32 = {
  Decide.handle choose { (resume) -> resume(true) } action {
    if choose_value() { 42 } else { 0 }
  }
}
