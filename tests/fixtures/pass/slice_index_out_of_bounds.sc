let slice = std.slice

let main(): i32 = {
  let values = [1, 2]
  let slice: borrow(slice(i32)) = borrow(values)
  slice[2]
}

test("slice_index_out_of_bounds.sc") {
  main() == 42
}
