let state(comptime s: type) = effect {
  let get(): s
  let put(move value: s): ()
}

let main(): i32 = {
  state(i32).handle get { (resume) -> resume(42) } action {
      state(i32).get()
    }
}
