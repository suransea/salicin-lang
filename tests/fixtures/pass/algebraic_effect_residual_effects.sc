let result = std.result

let throws = std.error.throws
let unsafe_effect = std.unsafe.unsafe_effect

let supply = effect {
  let seed(): i32
}

let ask = effect {
  let value(): i32 with(supply, throws(bool), unsafe_effect)
}

let request(): i32 with(ask, supply, throws(bool), unsafe_effect) = {
  ask.value()
}

let run(): i32 with(supply, throws(bool)) = {
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
