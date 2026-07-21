let Resource = struct(value: i32)
let Choice = enum { Pair(Resource, Resource), None }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.value
  }
}

let consume(move value: Resource): () = ()

let main(): i32 = Choice.Pair(Resource(1), Resource(0)) match {
  Pair(left, _) => do {
    consume(left)
    0
  },
  None => 0
}
