let state(comptime s: type) = effect {
  let get(): s
  let put(move value: s): ()
}

let read(): i32 with(state(i32)) = { state(i32).get() }
let write(value: i32): () with(state(i32)) = { state(i32).put(value) }

let main(): i32 = { 42 }

test("algebraic_effect_operations.sc") {
  main() == 42
}
