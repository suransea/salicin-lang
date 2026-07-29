let main(): i32 = {
  let mut values: array(i32)(2) = [40, 2]
  let mut iterator = values.iter(mut)()
  let first = iterator.next()
  let second = iterator.next()
  match first
    { some(value) -> value }
    { none -> 0 }
}
