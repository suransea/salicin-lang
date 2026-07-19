let Boxed = struct(value: i32)

let consume(move boxed: Boxed): () = {}

let main(): i32 = {
  let boxed = Boxed(value: 42)
  if true {
    consume(boxed)
  } else {
    let snapshot = boxed.value
  }
  42
}
