use std.vec.Vec

let main(): i32 = {
  let mut values: Vec(i32) = Vec(i32).new()
  values.push(42)
  let mutable = values.at(mut)(0)
  let shared = values.at(0)
  mutable + shared
}
