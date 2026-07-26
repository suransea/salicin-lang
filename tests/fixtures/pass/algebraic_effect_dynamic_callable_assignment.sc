let ask = effect {
  let value(): i32
}

let left(): i32 with(ask) = { ask.value() }
let right(): i32 with(ask) = { ask.value() + 1 }

let main(): i32 = {
  ask.handle value { (resume) -> resume(40) } action {
      let first: (): i32 with(ask) = if true { left } else { right }
      let second: (): i32 with(ask) = if true { right } else { left }
      let mut selected = first
      selected = second
      selected() + 1
    }
}

test("algebraic_effect_dynamic_callable_assignment.sc") {
  main() == 42
}
