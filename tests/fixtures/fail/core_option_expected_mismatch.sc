let option = std.option

let main(): i32 = {
  let value: option(bool) = option(i32).some(42)
  match value
    { some(flag) -> if flag { 42 } else { 0 } }
    { none -> 0 }
}
