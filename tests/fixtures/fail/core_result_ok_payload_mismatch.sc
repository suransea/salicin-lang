let Result = std.Result

let main(): i32 = {
  let value = Result(bool)(i32).Ok(true)
  match value
    { Ok(item) -> item }
    { Err(_) -> 0 }
}
