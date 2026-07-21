let option_tail(): Option(i32) = { Option(i32).Some(10) }

let option_return(): Option(i32) = {
  return Option(i32).Some(10)
}

let result_tail(): Result(i32, bool) = { Result(i32, bool).Ok(11) }

let result_return(): Result(i32, bool) = {
  return Result(i32, bool).Ok(11)
}

let main(): i32 = {
  let first = option_tail() ?? 0
  let second = option_return() ?? 0
  let third = result_tail() ?? 0
  let fourth = result_return() ?? 0
  first + second + third + fourth
}
