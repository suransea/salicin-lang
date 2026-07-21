let Resource = struct(value: i32)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let inspect(borrow resource: Resource): i32 = resource.value

let main(): i32 = inspect(Resource(42))
