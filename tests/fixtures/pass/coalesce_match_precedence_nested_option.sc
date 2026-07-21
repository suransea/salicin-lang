let main(): i32 = {
  let inner = Option(i32).Some(42)
  let outer = Option(Option(i32)).Some(inner)
  outer ?? Option(i32).None match {
    Some(value) => value,
    None => 0,
  }
}
