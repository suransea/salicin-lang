let main(): i32 = {
  let outer = Result(Result(i32, bool), bool).Err(false)
  let inner = outer ?? Result(i32, bool).Ok(42)
  inner ?? 0
}
