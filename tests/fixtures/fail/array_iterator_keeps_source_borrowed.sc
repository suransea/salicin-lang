let main(): i32 = {
  let mut values: array(i32)(2) = [40, 2]
  let iterator = values.iter()
  values[0] = 0
  for iterator { value -> () }
  42
}
