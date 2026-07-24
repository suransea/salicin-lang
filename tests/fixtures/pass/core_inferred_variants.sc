let Option = std.Option
let Result = std.Result

let main(): i32 = {
  let some = Option.Some(20)
  let none: Option(i32) = Option.None
  let ok = Result(E: bool).Ok(22)
  let err: Result(bool)(i32) = Result.Err(false)

  let from_some = match some
    { Some(value) -> value }
    { None -> 0 }
  let from_none = match none
    { Some(value) -> value }
    { None -> 0 }
  let from_ok = match ok
    { Ok(value) -> value }
    { Err(_) -> 0 }
  let from_err = match err
    { Ok(value) -> value }
    { Err(_) -> 0 }
  from_some + from_none + from_ok + from_err
}
