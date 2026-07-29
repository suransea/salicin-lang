let ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}

let choose: with(ask)(): i32 = {
  ask.value(left: 19) + ask.value(right: 23)
}

let main(): i32 = {
  ask.handle value { (left, resume) -> resume(left) } value { (right, resume) -> resume(right) } action {
      choose()
    }
}

test("algebraic_effect_named_overload.sc") {
  main() == 42
}
