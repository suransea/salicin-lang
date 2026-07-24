let main(): i32 = {
  let mut value = 0
  let mut total = 41
  do {
    value = value + 1
    if value < 3 {
      continue()
    }
    total = total + 1
  } while {
    value < 3
  }
  total
}
