let vec = alloc.vec.vec

let main(): i32 = {
  let values: vec(i32) = vec(i32).with_capacity(18446744073709551615)
  values.len()
  0
}

test("vec_capacity_overflow.sc") {
  std.test.assert(main() == 42)
}
