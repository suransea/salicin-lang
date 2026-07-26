let ask = effect {
  let value(): i32
}

let ask(): i32 with(ask) = {
  ask.value()
}

let invoke(action: (): i32 with(ask)): i32 with(ask) = {
  action()
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
      let selected = ask
      invoke(selected)
    }
}

test("algebraic_effect_static_higher_order.sc") {
  main() == 42
}
