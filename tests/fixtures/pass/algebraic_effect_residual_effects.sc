let result = core.result

let throwing = core.error.throwing
let unsafety = core.unsafe.unsafety

let supply = effect {
  let seed(): i32
}

let ask = effect {
  let value: with(supply, throwing(bool), unsafety)(): i32
}

let request: with(ask, supply, throwing(bool), unsafety)(): i32 = {
  ask.value()
}

let run: with(supply, throwing(bool))(): i32 = {
  unsafe {
    ask.handle value { (resume) -> resume(42) } action {
        request()
      }
  }
}

let main(): i32 = {
  let result: result(bool)(i32) = try {
    supply.handle seed { (resume) -> resume(0) } action { run() }
  }
  result ?? 0
}

test("algebraic_effect_residual_effects.sc") {
  std.test.assert(main() == 42)
}
