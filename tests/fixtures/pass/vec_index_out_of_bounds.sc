let vec = std.vec.vec

let main(): i32 = {
  let values = vec.new(i32)()
  values[0]
}

test("vec_index_out_of_bounds.sc") {
  main() == 42
}
