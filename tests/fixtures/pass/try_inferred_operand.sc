let option_none(): Option(i32) = Option.None.try

let result_err(): Result(i32, bool) = Result.Err(false).try

let result_ok(): Result(i32, bool) = Result.Ok(21).try

let local_result(): Result(bool, bool) = {
  let value = Result.Ok(21).try
  value == 21
}

let main(): i32 = {
  let first = option_none() ?? 10
  let second = result_err() ?? 11
  let third = result_ok() ?? 0
  let valid = local_result() ?? false
  if valid { first + second + third } else { 0 }
}
