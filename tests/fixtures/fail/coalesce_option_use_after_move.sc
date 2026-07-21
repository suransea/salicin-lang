let main(): i32 = {
  let value = Option(i32).Some(42)
  let answer = value ?? 0
  value match {
    Some(item) => item,
    None => answer,
  }
}
