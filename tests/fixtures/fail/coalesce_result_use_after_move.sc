let result = std.result

let main(): i32 = {
  let value = result(bool)(i32).ok(42)
  let answer = value ?? 0
  match value
    { ok(item) -> item }
    { err(_) -> answer }
}
