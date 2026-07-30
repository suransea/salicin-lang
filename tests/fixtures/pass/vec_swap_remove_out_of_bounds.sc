let vec = alloc.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  values.push(1)
  values.swap_remove(1)
}

test("vec_swap_remove_out_of_bounds.sc") {
  std.test.assert(main() == 42)
}
