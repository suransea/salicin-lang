use std.Option
use std.Result

let main(): i32 = {
  let option = Option.Some(20)
  let result = Result(E: bool).Ok(22)
  option! + result!
}
