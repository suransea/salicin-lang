use std.Result

let main(): i32 = {
  let value = Result(bool)(i32).Ok(42)
  value match {
    Ok(item) => item,
    Err(_) => 0,
  }
}
