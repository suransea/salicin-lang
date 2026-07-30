let vec = alloc.vec.vec

let main(): i32 = {
  let values: vec(i32) = vec(i32).new()
  let reference = values.at(0)
  reference
}

test("vec_at_out_of_bounds.sc") {
  std.test.assert(main() == 42)
}
