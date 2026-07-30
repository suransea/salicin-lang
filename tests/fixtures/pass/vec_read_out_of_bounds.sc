let vec = alloc.vec.vec

let main(): i32 = {
  let values: vec(i32) = vec(i32).new()
  values.read(0)
}

test("vec_read_out_of_bounds.sc") {
  std.test.assert(main() == 42)
}
