let State(S: type) = effect {
  let get(): S
}

let program(): i32 with(State(i32)) = {
  let answer = 1
  State(i32).get() + answer
}

let main(): i32 = {
  let answer = 40
  State(i32).handle get { (resume) -> resume(answer) } action {
    program() + 1
  }
}
