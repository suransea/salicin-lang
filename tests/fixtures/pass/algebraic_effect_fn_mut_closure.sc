let ask = effect {
  let value(): i32
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(10) } action {
      let mut total = 0
      let mut action: with(ask)((i32): i32)  = { (value: i32) ->
        total = total + value
        ask.value() + total
      }
      let first = action(1)
      let second = action(2)
      first + second + 18
    }
}

test("algebraic_effect_fn_mut_closure.sc") {
  std.test.assert(main() == 42)
}
