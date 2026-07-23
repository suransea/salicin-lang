let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trapped = 1 / self.value
  }}

let replace(target: borrow(mut)(Resource))(move replacement: Resource): () = {
  target = replacement
}

let main(): i32 = {
  let mut resource = Resource { value: 0 }
  replace(resource)(Resource { value: 1 })
  0
}
