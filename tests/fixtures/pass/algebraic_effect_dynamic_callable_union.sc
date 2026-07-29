let ask = effect {
  let choose(): bool
  let value(): i32
}

let left: with(ask)(): i32 = { ask.value() + 1 }
let middle: with(ask)(): i32 = { ask.value() + 2 }
let right: with(ask)(): i32 = { ask.value() + 3 }

let main(): i32 = {
  ask.handle choose { (resume) -> resume(false) } value { (resume) -> resume(39) } action {
      let first: with(ask)((): i32)  = if true { left } else { middle }
      let second: with(ask)((): i32)  = if false { middle } else { right }
      let combined: with(ask)((): i32)  = if ask.choose() { first } else { second }
      combined()
    }
}

test("algebraic_effect_dynamic_callable_union.sc") {
  main() == 42
}
