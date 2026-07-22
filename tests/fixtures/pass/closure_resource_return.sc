let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let consume(move resource: Resource): i32 = { resource.value }

let make() = {
  let resource = Resource { value: 1 }
  let closure = { (value: i32) -> consume(resource) + value }
  closure
}

let main(): i32 = {
  let closure = make()
  closure(41)
}
