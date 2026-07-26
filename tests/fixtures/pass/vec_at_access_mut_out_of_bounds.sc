let vec = std.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  let reference = values.at(mut)(0)
  reference
}

test("vec_at_access_mut_out_of_bounds.sc") {
  main() == 42
}
