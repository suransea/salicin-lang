let result = std.result

let main(): i32 = {
  let value = result(bool)(i32).ok(true)
  match value
    { ok(item) -> item }
    { err(_) -> 0 }
}
