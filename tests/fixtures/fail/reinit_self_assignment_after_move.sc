let Boxed = struct { value: i32 }

let consume(move boxed: Boxed): () = { () }

let main(): i32 = {
  let mut boxed = Boxed { value: 42 }
  consume(boxed)
  boxed = boxed
  boxed.value
}
