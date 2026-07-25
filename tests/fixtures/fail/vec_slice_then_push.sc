let Vec = std.vec.Vec

let main(): i32 = {
  let mut values = Vec.new(i32)()
  values.push(20)
  let slice = values.as_slice()
  values.push(22)
  slice.at(0)
}
