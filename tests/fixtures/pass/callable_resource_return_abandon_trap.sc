let Resource = struct(value: i32)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let finish(move resource: Resource)(value: i32): i32 = value

let make() = {
  let pending = finish(Resource(0))
  pending
}

let main(): i32 = {
  let pending = make()
  42
}
