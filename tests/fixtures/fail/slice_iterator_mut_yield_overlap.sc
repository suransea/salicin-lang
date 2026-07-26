let slice = std.slice

let main(): i32 = {
  let mut values: array(i32)(2) = [40, 2]
  let slice: borrow(mut)(slice(i32)) = borrow(mut)(values)
  let mut iterator = slice.iter(mut)()
  let first = iterator.next()
  let second = iterator.next()
  match first
    { some(value) -> value }
    { none -> 0 }
}
