let Vec = std.vec.Vec

let main(): i32 = {
  let mut values: Vec(i32) = Vec(i32).new()
  values.push(1)
  values.swap_remove(1)
}

test("vec_swap_remove_out_of_bounds.sc") {
  main() == 42
}
