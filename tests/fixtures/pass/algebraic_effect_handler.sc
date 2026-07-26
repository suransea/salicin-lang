let state(comptime s: type) = effect {
  let get(): s
  let put(move value: s): ()
}

let add_two(value: i32): i32 = { value + 2 }

let main(): i32 = {
  let mut state = 40
  state(i32).handle
    get { (resume) -> resume(state) }
    put { (value, resume) ->
      state = value;
      resume(())
    }
    action {
      state(i32).put(add_two(state(i32).get()))
      state(i32).get()
    }
}

test("algebraic_effect_handler.sc") {
  main() == 42
}
