let option = std.option

let main(): i32 = {
  let value = option(i32, bool).some(42)
  match value
    { some(item) -> item }
    { none -> 0 }
}
