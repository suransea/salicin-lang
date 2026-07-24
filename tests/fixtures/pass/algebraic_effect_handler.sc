let State(S: type) = effect {
  let get(): S
  let put(move value: S): ()
}

let add_two(value: i32): i32 = { value + 2 }

let main(): i32 = {
  let mut state = 40
  State(i32).handle
    get { (resume) -> resume(state) }
    put { (value, resume) ->
      state = value;
      resume(())
    }
    action {
    State(i32).put(add_two(State(i32).get()))
    State(i32).get()
  }
}
