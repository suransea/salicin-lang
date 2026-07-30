let result = core.result

let main(): i32 = {
  let value = result(bool)(i32).err(true)
  match value
    { ok(_) -> 0 }
    { err(failed) -> if failed { 42 } else { 0 } }
}

test("core_result_err.sc") {
  std.test.assert(main() == 42)
}
