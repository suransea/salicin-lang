use std.vec.Vec

let main(): i32 = {
  let mut values: Vec(i32) = Vec(i32).new()
  values.push(42)
  values.write(1)(0)
  0
}
