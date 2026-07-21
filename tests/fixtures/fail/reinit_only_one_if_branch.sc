let Boxed = struct(value: i32)

let consume(move boxed: Boxed): () = ()

let use_value(restore: bool): i32 = {
  let mut boxed = Boxed(0)
  consume(boxed)
  if restore {
    boxed = Boxed(42)
  }
  boxed.value
}

let main(): i32 = use_value(true)
