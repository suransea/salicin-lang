let vec = std.vec.vec

let main(): i32 = {
  let mut values: vec(i32) = vec(i32).new()
  values.push(42)
  values.write(1)(0)
  0
}

test("vec_write_out_of_bounds.sc") {
  main() == 42
}
