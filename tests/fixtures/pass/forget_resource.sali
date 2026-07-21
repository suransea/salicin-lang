let Resource = struct(value: i32)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trap = 1 / self.value
  }
}

let main(): i32 = {
  let resource = Resource(0)
  forget(resource)
  42
}
