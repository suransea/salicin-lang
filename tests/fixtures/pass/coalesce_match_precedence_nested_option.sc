let Option = std.Option

let main(): i32 = {
  let inner = Option(i32).Some(42)
  let outer = Option(Option(i32)).Some(inner)
  match outer ?? Option(i32).None
    { Some(value) -> value }
    { None -> 0 }
}
