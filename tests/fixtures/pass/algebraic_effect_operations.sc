let State(S: type) = effect {
  let get(): S
  let put(move value: S): ()
}

let read(): i32 with(State(i32)) = { State(i32).get() }
let write(value: i32): () with(State(i32)) = { State(i32).put(value) }

let main(): i32 = { 42 }
