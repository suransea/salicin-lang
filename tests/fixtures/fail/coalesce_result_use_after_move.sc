let Result = std.Result

let main(): i32 = {
  let value = Result(bool)(i32).Ok(42)
  let answer = value ?? 0
  match value
    { Ok(item) -> item }
    { Err(_) -> answer }
}
