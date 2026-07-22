use core.Result

let main(): i32 = {
  let value = Result(i32, bool).Err(true)
  value match {
    Ok(_) => 0,
    Err(failed) => if failed { 42 } else { 0 },
  }
}
