let tick = effect {
  let tick(): bool
}

let even: with(tick)(count: i32): bool = {
  if count == 0 { return(true) }
  if tick.tick() { odd(count - 1) } else { false }
}

let odd: with(tick)(count: i32): bool = {
  if count == 0 { return(false) }
  if tick.tick() { even(count - 1) } else { true }
}

let main(): i32 = {
  tick.handle tick { (resume) -> resume(true) } action {
      if odd(3) { 42 } else { 0 }
    }
}

test("algebraic_effect_mutual_answer.sc") {
  main() == 42
}
