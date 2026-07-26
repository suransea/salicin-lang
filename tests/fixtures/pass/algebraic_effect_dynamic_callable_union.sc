let ask = effect {
  let choose(): bool
  let value(): i32
}

let left(): i32 with(ask) = { ask.value() + 1 }
let middle(): i32 with(ask) = { ask.value() + 2 }
let right(): i32 with(ask) = { ask.value() + 3 }

let main(): i32 = {
  ask.handle choose { (resume) -> resume(false) } value { (resume) -> resume(39) } action {
      let first: (): i32 with(ask) = if true { left } else { middle }
      let second: (): i32 with(ask) = if false { middle } else { right }
      let combined: (): i32 with(ask) = if ask.choose() { first } else { second }
      combined()
    }
}

test("algebraic_effect_dynamic_callable_union.sc") {
  main() == 42
}
