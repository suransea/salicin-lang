let stop = effect {
  let stop(): bool
}

let main(): i32 = {
  stop.handle stop { (resume) -> 1 } action {
      let skipped = false && stop.stop()
      if skipped { 0 } else { 42 }
    }
}

test("algebraic_effect_short_circuit.sc") {
  main() == 42
}
