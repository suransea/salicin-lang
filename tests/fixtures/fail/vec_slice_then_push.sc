let vec = std.vec.vec

let main(): i32 = {
  let mut values = vec.new(t: i32)()
  values.push(20)
  let slice = values.as_slice()
  values.push(22)
  slice.at(0)
}
