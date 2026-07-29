let stop = effect {
  let stop(): i32
}

let program: with(stop)(): i32 = {
  let value = stop.stop()
  value + 1
}

let main(): i32 = {
  let result = stop.handle stop { (resume) -> 40 } action {
      program() + 1
    }
  result + 2
}

test("algebraic_effect_cross_function_abort.sc") {
  main() == 42
}
