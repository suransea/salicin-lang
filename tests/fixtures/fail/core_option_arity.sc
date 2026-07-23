use std.Option

let main(): i32 = {
  let value = Option(i32, bool).Some(42)
  value match {
    Some(item) => item,
    None => 0,
  }
}
