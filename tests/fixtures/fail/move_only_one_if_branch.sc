let boxed = struct { value: i32 }

let consume(move boxed: boxed): () = { () }

let use_value(take: bool): i32 = {
  let boxed = boxed { value: 42 }
  if take {
    consume(boxed)
  }
  boxed.value
}

let main(): i32 = { use_value(false) }
