let result = core.result

let throwing = core.error.throwing
let unsafety = core.unsafe.unsafety

let supply = effect {
  let seed(): i32
}

let ask = effect {
  let value(): i32 with(supply, throwing(bool), unsafety)
}

let request(): i32 with(ask, supply, throwing(bool), unsafety) = {
  ask.value()
}

let run(): i32 with(supply, throwing(bool)) = {
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
  main() == 42
}
