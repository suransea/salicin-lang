let main(): i32 = {
  let value = 42
  let alias: borrow(i32) = value
  alias
}
