let ask = effect {
  let value(): i32
}

let ask: with(ask)(): i32 = {
  ask.value()
}

let invoke: with(ask)(action: with(ask)((): i32)): i32 = {
  action()
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
      let selected = ask
      invoke(selected)
    }
}

test("algebraic_effect_static_higher_order.sc") {
  std.test.assert(main() == 42)
}
