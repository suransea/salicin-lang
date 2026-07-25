let Vec = std.vec.Vec

let main(): i32 = {
  let values = Vec.new(i32)()
  values[0]
}

test("vec_index_out_of_bounds.sc") {
  main() == 42
}
