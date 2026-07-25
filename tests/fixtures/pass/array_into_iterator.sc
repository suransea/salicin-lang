let main(): i32 = {
  let values: Array(i32)(3) = [10, 11, 21]
  let mut total = 0
  for values { value ->
    total = total + value
  }
  total
}
