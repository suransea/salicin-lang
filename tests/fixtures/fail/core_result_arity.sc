let Result = std.Result

let main(): i32 = {
  let value: Result(i32) = Result(i32).Ok(42)
  match value
    { Ok(item) -> item }
    { Err(_) -> 0 }
}
