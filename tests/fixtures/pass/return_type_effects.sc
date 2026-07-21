let read(fail: bool): i32 with(throws(bool), unsafe) = {
  if fail { throw true }
  42
}

let main(): i32 = {
  let result: Result(i32, bool) = try { unsafe { read(false) } }
  result ?? 0
}
