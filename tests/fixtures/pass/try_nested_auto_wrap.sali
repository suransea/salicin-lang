let nested_option(): Result(Option(i32), bool) = Option.None

let nested_result(): Option(Result(i32, bool)) = Result.Err(false)

let main(): i32 = {
  let first = nested_option() ?? Option(i32).Some(20)
  let second = nested_result() ?? Result(i32, bool).Ok(22)
  (first ?? 20) + (second ?? 22)
}
