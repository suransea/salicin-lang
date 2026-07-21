let main(): i32 = {
  let value: Option(bool) = Option(i32).Some(42)
  value match {
    Some(flag) => if flag { 42 } else { 0 },
    None => 0,
  }
}
