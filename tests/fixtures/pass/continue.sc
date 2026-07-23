let main(): i32 = {
  let mut value = 0
  let mut total = 30
  while { value < 5 } {
    value = value + 1
    if value < 3 {
      continue
    }
    total = total + value
  }
  total
}
