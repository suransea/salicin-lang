let Vec = std.vec.Vec

let main(): i32 = {
  let mut values: Vec(i32) = Vec(i32).new()
  values.push(42)
  values.swap(0, 1)
  42
}

test("vec_swap_right_out_of_bounds.sc") {
  main() == 42
}
