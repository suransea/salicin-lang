let Resource = struct(value: i32)
let Pair = struct(left: Resource, right: Resource)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.value
  }
}

let consume(move value: Resource): () = ()

let main(): i32 = {
  let pair = Pair(Resource(1), Resource(0))
  consume(pair.left)
  0
}
