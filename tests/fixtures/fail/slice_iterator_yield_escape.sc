let slice = std.slice

let escape(): borrow(i32) = {
  let values: array(i32)(1) = [42]
  let slice: borrow(slice(i32)) = borrow(values)
  let mut iterator = slice.iter()
  match iterator.next()
    { some(value) -> value }
    { none -> unsafe { raw_trap() } }
}

let main(): i32 = { 0 }
