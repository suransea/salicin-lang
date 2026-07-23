let Resource = struct { value: i32 }
let Choice = enum { Pair(Resource, Resource), None }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let consume(move value: Resource): () = { () }

let inspect(move choice: Choice): i32 = { choice match {
  Pair(left, _) => do {
    consume(left)
    42
  },
  None => 0
}
}

let escape(move choice: Choice): i32 = { choice match {
  Pair(left, _) => do {
    consume(left)
    return 42
  },
  None => 0
}
}

let main(): i32 = {
  let first = inspect(Choice.Pair(Resource { value: 1 }, Resource { value: 1 }))
  let second = escape(Choice.Pair(Resource { value: 1 }, Resource { value: 1 }))
  first + second - 42
}
