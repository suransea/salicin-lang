let Vec = std.vec.Vec

let main(): i32 = {
  let mut values = Vec.new(i32)()
  values.push(10)
  values.push(11)
  values.push(21)
  let mut total = 0
  for values { value ->
    total = total + value
  }
  total
}
