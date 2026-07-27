let result = core.result

let main(): i32 = {
  let value = result(i32)(i32).err(true)
  match value
    { ok(item) -> item }
    { err(_) -> 0 }
}
