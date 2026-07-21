let State(S: type) = effect {
  let get(): S
}

let read(): i32 = { State(i32).get() }
let main(): i32 = { read() }
