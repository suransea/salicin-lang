let slice = core.memory.slice

let main(): i32 = {
  let values = [1, 2]
  let slice: borrow(slice(i32)) = borrow(values)
  let item = slice.at(2)
  item
}

test("slice_out_of_bounds.sc") {
  std.test.assert(main() == 42)
}
