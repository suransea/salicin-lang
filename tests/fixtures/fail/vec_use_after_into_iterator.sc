let Vec = std.vec.Vec

let main(): i32 = {
  let mut values = Vec(i32).new()
  values.push(42)
  for values { value -> () }
  values.len()
}
