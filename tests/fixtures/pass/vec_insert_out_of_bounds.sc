let vec = std.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  values.insert(1)(42)
  42
}

test("vec_insert_out_of_bounds.sc") {
  main() == 42
}
