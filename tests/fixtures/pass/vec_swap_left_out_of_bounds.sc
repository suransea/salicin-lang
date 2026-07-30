let vec = alloc.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  values.push(42)
  values.swap(1, 0)
  42
}

test("vec_swap_left_out_of_bounds.sc") {
  std.test.assert(main() == 42)
}
