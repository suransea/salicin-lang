let vec = alloc.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  values.push(1)
  values.push(2)
  values.copy_within(0, 2, 1)
  0
}
