let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.value
  }}

let replace(borrow(mut) target: Resource)(move replacement: Resource): () = {
  target = replacement
}

let main(): i32 = {
  let mut resource = Resource { value: 0 }
  replace(resource)(Resource { value: 1 })
  0
}
