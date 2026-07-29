let abort = effect {
  let stop(value: i32): never
}

let fail: with(abort)(): never = {
  abort.stop(42)
}

let main(): i32 = {
  abort.handle stop { (value) -> value } action {
      fail()
    }
}

test("algebraic_effect_never_abort.sc") {
  main() == 42
}
