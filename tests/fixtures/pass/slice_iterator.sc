let Slice = std.Slice

let main(): i32 = {
  let values: Array(i32)(3) = [10, 11, 21]
  let slice: borrow(Slice(i32)) = borrow(values)
  let mut total = 0
  for slice.iter() { value ->
    total = total + value
  }
  total
}
