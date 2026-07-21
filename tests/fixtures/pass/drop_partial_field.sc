let Resource = struct(value: i32)
let Pair = struct(left: Resource, right: Resource)
let Nested = struct(pair: Pair, tail: Resource)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move value: Resource): () = { () }

let conditional(flag: bool): () = {
  let pair = Pair(Resource(1), Resource(1))
  if flag { consume(pair.left) }
}

let rebuild(): () = {
  let mut pair = Pair(Resource(1), Resource(1))
  consume(pair.left)
  pair.left = Resource(1)
}

let conditional_rebuild(flag: bool): () = {
  let mut pair = Pair(Resource(1), Resource(1))
  if flag { consume(pair.left) }
  pair.left = Resource(1)
}

let nested(): () = {
  let value = Nested(Pair(Resource(1), Resource(1)), Resource(1))
  consume(value.pair.left)
}

let main(): i32 = {
  conditional(true)
  conditional(false)
  rebuild()
  conditional_rebuild(true)
  conditional_rebuild(false)
  nested()
  42
}
