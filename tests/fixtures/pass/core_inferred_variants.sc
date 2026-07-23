use std.Option
use std.Result

let main(): i32 = {
  let some = Option.Some(20)
  let none: Option(i32) = Option.None
  let ok = Result(E: bool).Ok(22)
  let err: Result(bool)(i32) = Result.Err(false)

  let from_some = some match {
    Some(value) => value,
    None => 0,
  }
  let from_none = none match {
    Some(value) => value,
    None => 0,
  }
  let from_ok = ok match {
    Ok(value) => value,
    Err(_) => 0,
  }
  let from_err = err match {
    Ok(value) => value,
    Err(_) => 0,
  }
  from_some + from_none + from_ok + from_err
}
