use std.Option

let main(): i32 = {
  let value = Option(i32).Some(42)
  match value
    { Some(item) -> item }
    { None -> 0 }
}
