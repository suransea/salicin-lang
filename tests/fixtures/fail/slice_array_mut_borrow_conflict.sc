let slice = std.slice

let main(): i32 = {
  let mut values = [20, 22]
  let slice: borrow(slice(i32)) = borrow(values)
  values[0] = 0
  slice.at(1)
}
