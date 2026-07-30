let step = effect {
  let next(value: i32): i32
}

let combine: with(step)(left: i32, right: i32): i32 = {
  left - right + 46
}

let main(): i32 = {
  step.handle next { (value, resume) -> resume(value) } action {
      combine(step.next(19), step.next(23))
    }
}

test("algebraic_effect_call_arguments.sc") {
  std.test.assert(main() == 42)
}
