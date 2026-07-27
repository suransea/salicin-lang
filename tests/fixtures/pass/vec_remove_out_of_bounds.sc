let vec = alloc.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  values.remove(0)
}

test("vec_remove_out_of_bounds.sc") {
  main() == 42
}
