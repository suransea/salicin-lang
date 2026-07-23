let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let consume(move resource: Resource): () = { () }

let finish(move resource: Resource)(value: i32): i32 = {
  consume(resource)
  value
}

let make() = {
  let pending = finish(Resource { value: 1 })
  pending
}

let main(): i32 = {
  let original = make()
  let pending = original
  pending(42)
}
