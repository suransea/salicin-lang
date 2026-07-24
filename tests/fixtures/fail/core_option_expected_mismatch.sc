let Option = std.Option

let main(): i32 = {
  let value: Option(bool) = Option(i32).Some(42)
  match value
    { Some(flag) -> if flag { 42 } else { 0 } }
    { None -> 0 }
}
