use core.Result

let main(): i32 = {
  let value: Result(i32, bool) = Result(i32, i32).Err(42)
  value match {
    Ok(item) => item,
    Err(_) => 0,
  }
}
