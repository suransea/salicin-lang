let result = std.result

let main(): i32 = {
  let value: result(bool)(i32) = result(i32)(i32).err(42)
  match value
    { ok(item) -> item }
    { err(_) -> 0 }
}
