let boxed = struct { value: i32 }

let consume(move boxed: boxed): () = { () }

let main(): i32 = {
  let mut boxed = boxed { value: 42 }
  consume(boxed)
  boxed = boxed
  boxed.value
}
