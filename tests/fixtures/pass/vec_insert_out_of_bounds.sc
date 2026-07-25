let Vec = std.vec.Vec

let main(): i32 = {
  let mut values: Vec(i32) = Vec(i32).new()
  values.insert(1)(42)
  42
}

test("vec_insert_out_of_bounds.sc") {
  main() == 42
}
