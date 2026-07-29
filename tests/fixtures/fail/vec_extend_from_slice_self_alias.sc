let vec = alloc.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  values.push(42)
  let source = values.as_slice()
  values.extend_from_slice(source)
  0
}
