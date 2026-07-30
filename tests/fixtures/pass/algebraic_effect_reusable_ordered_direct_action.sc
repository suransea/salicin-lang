let ask = effect {
  let value(): i32
}

let run(seed: i32)(move action: with(ask)((): i32)): i32 = {
  ask.handle value { (resume) -> resume(20) } action {
      action() + seed
    }
}

let prepare(order: borrow(mut)(i32)): i32 = {
  order = order + 1
  20
}

let main(): i32 = {
  let mut order = 0
  run(prepare(order)) { () ->
      order = order * 2
      ask.value() + order
    }
}

test("algebraic_effect_reusable_ordered_direct_action.sc") {
  std.test.assert(main() == 42)
}
