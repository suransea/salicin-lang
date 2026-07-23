let main(): i32 = {
  let value = 42
  let alias: borrow(R)(i32) = borrow(value)
  alias
}
