let ask = effect {
  let value(): i32
}

let run()(move action: with(ask)((): i32)): i32 = {
  ask.handle value { (resume) -> resume(10) } action {
      action()
    }
}

let main(): i32 = {
  let mut base = 31
  run() { () ->
      base = base + 1
      ask.value() + base
    }
}

test("algebraic_effect_reusable_direct_action.sc") {
  std.test.assert(main() == 42)
}
