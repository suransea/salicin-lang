let Boxed = struct { value: i32 }

let consume(move boxed: Boxed): () = {}

let use_value(take_first: bool): i32 = {
  let boxed = Boxed { value: 42 }
  if take_first {
    consume(boxed)
  } else {
    consume(boxed)
  }
  boxed.value
}

let main(): i32 = { use_value(false) }
