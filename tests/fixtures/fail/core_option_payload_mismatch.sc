let option = std.option

let main(): i32 = {
  let value = option(i32).some(true)
  match value
    { some(item) -> item }
    { none -> 0 }
}
