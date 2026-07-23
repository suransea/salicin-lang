let Resource = struct { value: i32 }
let Holder = struct { resource: Resource, tail: Resource }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trapped = 1 / self.value
  }}

let replace(target: borrow(mut)(Holder))(move replacement: Resource): () = {
  target.resource = replacement
}

let main(): i32 = {
  let mut holder = Holder { resource: Resource { value: 0 }, tail: Resource { value: 1 } }
  replace(holder)(Resource { value: 1 })
  0
}
