let Vec = std.vec.Vec

let main(): i32 = {
  let values: Vec(i32) = Vec(i32).new()
  let reference = values.at(0)
  reference
}
