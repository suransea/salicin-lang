use std.Option
use std.Result

let main(): i32 = {
  let option_number = Option(i32).Some(20)
  let option_flag = Option(bool).Some(true)
  let result_ok = Result(bool)(i32).Ok(7)
  let result_err = Result(i32)(bool).Err(5)

  let first = option_number match {
    Some(value) => value,
    None => 0,
  }
  let second = option_flag match {
    Some(value) => if value { 10 } else { 0 },
    None => 0,
  }
  let third = result_ok match {
    Ok(value) => value,
    Err(_) => 0,
  }
  let fourth = result_err match {
    Ok(_) => 0,
    Err(value) => value,
  }
  first + second + third + fourth
}
