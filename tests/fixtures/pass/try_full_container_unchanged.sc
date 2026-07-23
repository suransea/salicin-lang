use std.Option
use std.Result

let option_tail(): Option(i32) = { Option(i32).Some(10) }

let option_return(): Option(i32) = {
  return Option(i32).Some(10)
}

let result_tail(): Result(bool)(i32) = { Result(bool)(i32).Ok(11) }

let result_return(): Result(bool)(i32) = {
  return Result(bool)(i32).Ok(11)
}

let main(): i32 = {
  let first = option_tail() ?? 0
  let second = option_return() ?? 0
  let third = result_tail() ?? 0
  let fourth = result_return() ?? 0
  first + second + third + fourth
}
