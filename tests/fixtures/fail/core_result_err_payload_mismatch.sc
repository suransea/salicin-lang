use core.Result

let main(): i32 = {
  let value = Result(i32, i32).Err(true)
  value match {
    Ok(item) => item,
    Err(_) => 0,
  }
}
