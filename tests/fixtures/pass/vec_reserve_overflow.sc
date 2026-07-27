let vec = alloc.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  values.push(1)
  values.reserve(18446744073709551615)
  42
}

test("vec_reserve_overflow.sc") {
  main() == 42
}
