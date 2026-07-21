let Resource = struct(value: i32)
let Holder = struct(resource: Resource, tail: Resource)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.value
  }
}

let replace(borrow(mut) target: Holder)(move replacement: Resource): () = {
  target.resource = replacement
}

let main(): i32 = {
  let mut holder = Holder(Resource(0), Resource(1))
  replace(holder)(Resource(1))
  0
}
