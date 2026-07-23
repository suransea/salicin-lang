use std.Result

let main(): i32 = {
  let value: Result(bool)(i32) = Result(i32)(i32).Err(42)
  value match {
    Ok(item) => item,
    Err(_) => 0,
  }
}
