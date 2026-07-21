use alloc.vec.Vec

let Resource = struct(value: i32)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {}
}

let main(): i32 = {
  let mut values: Vec(Resource) = Vec(Resource).new()
  let resource = Resource(42)
  values.push(resource)
  resource.value
}
