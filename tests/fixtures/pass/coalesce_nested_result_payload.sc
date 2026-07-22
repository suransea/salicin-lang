use core.Result

let main(): i32 = {
  let outer = Result(bool)(Result(bool)(i32)).Err(false)
  let inner = outer ?? Result(bool)(i32).Ok(42)
  inner ?? 0
}
