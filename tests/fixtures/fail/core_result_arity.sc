let result = std.result

let main(): i32 = {
  let value: result(i32) = result(i32).ok(42)
  match value
    { ok(item) -> item }
    { err(_) -> 0 }
}
