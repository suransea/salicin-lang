let Resource = struct(value: i32)
let Choice = enum { Pair(Resource, i32), None }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move value: Resource): () = ()

let choose(move choice: Choice): i32 = choice match {
  Pair(resource, 42) => do {
    consume(resource)
    21
  },
  Pair(resource, _) => do {
    consume(resource)
    21
  },
  None => 0,
}

let main(): i32 = {
  choose(Choice.Pair(Resource(1), 0)) +
    choose(Choice.Pair(Resource(1), 42))
}
