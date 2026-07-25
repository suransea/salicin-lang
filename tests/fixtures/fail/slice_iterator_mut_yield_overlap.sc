let Slice = std.Slice

let main(): i32 = {
  let mut values: Array(i32)(2) = [40, 2]
  let slice: borrow(mut)(Slice(i32)) = borrow(mut)(values)
  let mut iterator = slice.iter(mut)()
  let first = iterator.next()
  let second = iterator.next()
  match first
    { Some(value) -> value }
    { None -> 0 }
}
