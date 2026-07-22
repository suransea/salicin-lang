let Resource = struct { value: i32 }
let Holder = struct { resource: Resource, tail: Resource }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.value
  }}

let replace(borrow(mut) target: Holder)(move replacement: Resource): () = {
  target.resource = replacement
}

let main(): i32 = {
  let mut holder = Holder { resource: Resource { value: 0 }, tail: Resource { value: 1 } }
  replace(holder)(Resource { value: 1 })
  0
}
