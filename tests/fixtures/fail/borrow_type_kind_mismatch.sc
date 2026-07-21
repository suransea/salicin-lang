let main(): i32 = {
  let value = 42
  let alias: borrow(mut) i32 = borrow value
  alias
}
