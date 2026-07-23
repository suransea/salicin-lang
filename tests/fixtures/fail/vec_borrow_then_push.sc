use std.vec.Vec

let Resource = struct { value: i32 }

let main(): i32 = {
  let mut values: Vec(Resource) = Vec(Resource).new()
  values.push(Resource { value: 20 })
  let reference = values.at(0)
  values.push(Resource { value: 22 })
  reference.value
}
