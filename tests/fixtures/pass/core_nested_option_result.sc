use std.Option
use std.Result

let main(): i32 = {
  let inner = Result(bool)(i32).Ok(42)
  let outer = Option(Result(bool)(i32)).Some(inner)
  outer match {
    Some(result) => result match {
      Ok(value) => value,
      Err(_) => 0,
    },
    None => 0,
  }
}
