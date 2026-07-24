use std.Option
use std.Result

let main(): i32 = {
  let option_number = Option(i32).Some(20)
  let option_flag = Option(bool).Some(true)
  let result_ok = Result(bool)(i32).Ok(7)
  let result_err = Result(i32)(bool).Err(5)

  let first = match option_number
    { Some(value) -> value }
    { None -> 0 }
  let second = match option_flag
    { Some(value) -> if value { 10 } else { 0 } }
    { None -> 0 }
  let third = match result_ok
    { Ok(value) -> value }
    { Err(_) -> 0 }
  let fourth = match result_err
    { Ok(_) -> 0 }
    { Err(value) -> value }
  first + second + third + fourth
}
