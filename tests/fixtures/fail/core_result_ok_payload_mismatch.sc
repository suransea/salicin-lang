use std.Result

let main(): i32 = {
  let value = Result(bool)(i32).Ok(true)
  value match {
    Ok(item) => item,
    Err(_) => 0,
  }
}
