let vec = std.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  values.push(42)
  let mutable = values.at(mut)(0)
  let shared = values.at(0)
  mutable + shared
}
