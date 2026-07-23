let Resource = struct { value: i32 }
let Holder = struct { resource: Resource, tail: Resource }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let replace_root(target: borrow(mut)(Resource))(move replacement: Resource): () = {
  target = replacement
}

let replace_field(target: borrow(mut)(Holder))(move replacement: Resource): () = {
  target.resource = replacement
}

let main(): i32 = {
  let mut resource = Resource { value: 1 }
  replace_root(resource)(Resource { value: 1 })
  let mut holder = Holder { resource: Resource { value: 1 }, tail: Resource { value: 1 } }
  replace_field(holder)(Resource { value: 1 })
  42
}
