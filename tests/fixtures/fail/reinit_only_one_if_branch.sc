let boxed = struct { value: i32 }

let consume(move boxed: boxed): () = { () }

let use_value(restore: bool): i32 = {
  let mut boxed = boxed { value: 0 }
  consume(boxed)
  if restore {
    boxed = boxed { value: 42 }
  }
  boxed.value
}

let main(): i32 = { use_value(true) }
