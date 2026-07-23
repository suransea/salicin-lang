use std.Option

let main(): i32 = {
  let value: Option(i32) = Option.None
  value match {
    Some(_) => 0,
    None => 42,
  }
}
