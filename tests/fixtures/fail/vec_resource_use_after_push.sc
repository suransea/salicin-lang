use std.vec.Vec

let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {}}

let main(): i32 = {
  let mut values: Vec(Resource) = Vec(Resource).new()
  let resource = Resource { value: 42 }
  values.push(resource)
  resource.value
}
