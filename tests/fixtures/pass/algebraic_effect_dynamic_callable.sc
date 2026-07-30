let ask = effect {
  let value(): i32
}

let left: with(ask)(): i32 = {
  ask.value()
}

let right: with(ask)(): i32 = {
  ask.value() + 1
}

let fallback: with(ask)(): i32 = {
  ask.value()
}

let invoke: with(ask)(action: with(ask)((): i32)): i32 = {
  action()
}

let finish: with(ask)(value: i32): i32 = {
  value + 1
}

let select: with(ask)(mode: i32): i32 = {
  let action: with(ask)((): i32)  = if mode == 0 { left } else if mode == 1 { right } else { fallback }
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
  std.test.assert(main() == 42)
}
