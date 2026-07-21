let option_value(): Option(i32) = {
  return 20
}

let result_value(): Result(i32, bool) = {
  return 22
}

let main(): i32 = (option_value() ?? 0) + (result_value() ?? 0)
