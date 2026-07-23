let consume(move value: i32): () = { () }

let main(): i32 = {
  let value = 42
  while { true } {
    consume(value)
  }
  0
}
