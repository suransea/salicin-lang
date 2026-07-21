let State(S: type) = effect {
  let get(): S
  let put(move value: S): ()
}

let main(): i32 = {
  let mut state = 40
  State(i32).handle(
    get: { (resume) -> resume(state) },
    put: { (value, resume) ->
      state = value;
      resume(())
    },
  ) {
    let value = State(i32).get()
    State(i32).put(value + 2)
    State(i32).get()
  }
}
