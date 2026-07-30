let abort = effect {
  let stop(): i32
}

let main(): i32 = {
  let mut reached = 0
  let result = abort.handle stop { (resume) -> 42 } action {
      let value = abort.stop()
      reached = 1;
      value
    }
  result + reached
}

test("algebraic_effect_abort.sc") {
  std.test.assert(main() == 42)
}
