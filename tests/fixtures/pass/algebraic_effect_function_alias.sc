let ask = effect {
  let value(): i32
}

let ask: with(ask)(): i32 = {
  ask.value()
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
      let action = ask
      let forwarded = action
      forwarded()
    }
}

test("algebraic_effect_function_alias.sc") {
  std.test.assert(main() == 42)
}
