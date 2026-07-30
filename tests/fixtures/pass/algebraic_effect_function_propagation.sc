let state(comptime s: type) = effect {
  let get(): s
}

let program: with(state(i32))(): i32 = {
  let answer = 1
  state(i32).get() + answer
}

let main(): i32 = {
  let answer = 40
  state(i32).handle get { (resume) -> resume(answer) } action {
      program() + 1
    }
}

test("algebraic_effect_function_propagation.sc") {
  std.test.assert(main() == 42)
}
