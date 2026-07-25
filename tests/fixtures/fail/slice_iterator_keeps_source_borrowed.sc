let Slice = std.Slice

let main(): i32 = {
  let mut values: Array(i32)(2) = [40, 2]
  let slice: borrow(Slice(i32)) = borrow(values)
  let iterator = slice.iter()
  values[0] = 0
  for iterator { value -> () }
  42
}
