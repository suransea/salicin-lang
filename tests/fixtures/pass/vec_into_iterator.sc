let vec = std.vec.vec

let main(): i32 = {
  let mut values = vec.new(t: i32)()
  values.push(10)
  values.push(11)
  values.push(21)
  let mut total = 0
  for values { value ->
    total = total + value
  }
  total
}

test("vec_into_iterator.sc") {
  main() == 42
}
