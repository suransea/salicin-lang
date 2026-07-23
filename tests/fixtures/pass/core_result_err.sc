use std.Result

let main(): i32 = {
  let value = Result(bool)(i32).Err(true)
  value match {
    Ok(_) => 0,
    Err(failed) => if failed { 42 } else { 0 },
  }
}
