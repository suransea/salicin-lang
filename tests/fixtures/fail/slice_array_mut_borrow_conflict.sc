let Slice = std.Slice

let main(): i32 = {
  let mut values = [20, 22]
  let slice: borrow(Slice(i32)) = borrow(values)
  values[0] = 0
  slice.at(1)
}
