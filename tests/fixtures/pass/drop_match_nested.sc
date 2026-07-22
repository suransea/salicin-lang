let Resource = struct { value: i32 }
let Bundle = struct { left: Resource, right: Resource }
let Choice = enum { Some(Bundle, Resource), None }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let consume(move value: Resource): () = { () }

let inspect(move choice: Choice): i32 = { choice match {
  Some(Bundle(left: left, right: _), _) => do {
    consume(left)
    return 42
  },
  None => 0
}
}

let main(): i32 = { inspect(
  Choice.Some(Bundle { left: Resource { value: 1 }, right: Resource { value: 1 } }, Resource { value: 1 })
)
}
