let ask = effect {
  let value(): i32
}

let left(): i32 with(ask) = {
  ask.value()
}

let right(): i32 with(ask) = {
  ask.value() + 1
}

let fallback(): i32 with(ask) = {
  ask.value()
}

let invoke(action: (): i32 with(ask)): i32 with(ask) = {
  action()
}

let finish(value: i32): i32 with(ask) = {
  value + 1
}

let select(mode: i32): i32 with(ask) = {
  let action: (): i32 with(ask) = if mode == 0 { left } else if mode == 1 { right } else { fallback }
  let direct = finish(action())
  let higher = invoke(action)
  direct + higher + 1
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(20) } action {
      select(2)
    }
}

test("algebraic_effect_dynamic_callable.sc") {
  main() == 42
}
