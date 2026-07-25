let Stop = effect {
  let stop(): bool
}

let main(): i32 = {
  Stop.handle stop { (resume) -> 1 } action {
    let skipped = false && Stop.stop()
    if skipped { 0 } else { 42 }
  }
}

test("algebraic_effect_short_circuit.sc") {
  main() == 42
}
