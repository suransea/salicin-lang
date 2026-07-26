let slice = std.slice

let main(): i32 = {
  let mut values: array(i32)(2) = [40, 2]
  let slice: borrow(slice(i32)) = borrow(values)
  let iterator = slice.iter()
  values[0] = 0
  for iterator { value -> () }
  42
}
