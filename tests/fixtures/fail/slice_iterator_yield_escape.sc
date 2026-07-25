let Slice = std.Slice

let escape(): borrow(i32) = {
  let values: Array(i32)(1) = [42]
  let slice: borrow(Slice(i32)) = borrow(values)
  let mut iterator = slice.iter()
  match iterator.next()
    { Some(value) -> value }
    { None -> unsafe { raw_trap() } }
}

let main(): i32 = { 0 }
