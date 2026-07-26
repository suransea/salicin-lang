let tick = effect {
  let tick(): i32
}

let even(count: i32): i32 with(tick) = {
  if count == 0 { return(0) }
  tick.tick() + odd(count - 1)
}

let odd(count: i32): i32 with(tick) = {
  if count == 0 { return(0) }
  tick.tick() + even(count - 1)
}

let main(): i32 = {
  let value = 14
  tick.handle tick { (resume) -> resume(value) } action {
      even(3)
    }
}

test("algebraic_effect_mutual_recursion.sc") {
  main() == 42
}
