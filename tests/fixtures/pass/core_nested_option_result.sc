let Option = std.Option
let Result = std.Result

let main(): i32 = {
  let inner = Result(bool)(i32).Ok(42)
  let outer = Option(Result(bool)(i32)).Some(inner)
  match outer
    { Some(result) -> match result
      { Ok(value) -> value }
      { Err(_) -> 0 } }
    { None -> 0 }
}
