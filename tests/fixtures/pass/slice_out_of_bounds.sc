let Slice = std.Slice

let main(): i32 = {
  let values = [1, 2]
  let slice: borrow(Slice(i32)) = borrow(values)
  let item = slice.at(2)
  item
}

test("slice_out_of_bounds.sc") {
  main() == 42
}
