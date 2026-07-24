let Result = std.Result

let main(): i32 = {
  let value = Result(i32)(i32).Err(true)
  match value
    { Ok(item) -> item }
    { Err(_) -> 0 }
}
