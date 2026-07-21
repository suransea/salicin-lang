let main(): i32 = {
  let value = Result(i32, bool).Ok(42)
  let answer = value ?? 0
  value match {
    Ok(item) => item,
    Err(_) => answer,
  }
}
