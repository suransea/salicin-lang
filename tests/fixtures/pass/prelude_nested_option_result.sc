let main(): i32 = {
  let inner = Result(i32, bool).Ok(42)
  let outer = Option(Result(i32, bool)).Some(inner)
  outer match {
    Some(result) => result match {
      Ok(value) => value,
      Err(_) => 0,
    },
    None => 0,
  }
}
