let Resource = struct(value: i32)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.value
  }
}

let consume(move value: Resource): () = { () }

let main(): i32 = {
  let resource = Resource(0)
  let once = { (value: i32) -> do {
    consume(resource)
    value
  }}
  once(return 0)
}
