let State(S: type) = effect {
  let get(): S
  let put(move value: S): ()
}

let main(): i32 = {
  State(i32).handle get { (resume) -> resume(42) } action {
    State(i32).get()
  }
}
