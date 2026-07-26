let vec = std.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  values.replace(0)(42)
}

test("vec_replace_out_of_bounds.sc") {
  main() == 42
}
