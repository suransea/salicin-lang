let Resource = struct(value: i32)
let Holder = struct(resource: Resource, tail: Resource)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let replace_root(borrow(mut) target: Resource)(move replacement: Resource): () = {
  target = replacement
}

let replace_field(borrow(mut) target: Holder)(move replacement: Resource): () = {
  target.resource = replacement
}

let main(): i32 = {
  let mut resource = Resource(1)
  replace_root(resource)(Resource(1))
  let mut holder = Holder(Resource(1), Resource(1))
  replace_field(holder)(Resource(1))
  42
}
