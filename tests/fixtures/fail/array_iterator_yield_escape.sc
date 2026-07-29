let escape(): borrow(i32) = {
  let values: array(i32)(1) = [42]
  let mut iterator = values.iter()
  match iterator.next()
    { some(value) -> value }
    { none -> unsafe { raw_trap() } }
}

let main(): i32 = { 0 }
