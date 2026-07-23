let main(): i32 = {
  let value = 42
  let alias: borrow(i64) = borrow(value)
  0
}
