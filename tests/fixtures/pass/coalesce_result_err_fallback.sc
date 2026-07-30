let result = core.result

let fallback(count: borrow(mut)(i32)): i32 = {
  count = count + 1
  42
}

let main(): i32 = {
  let mut count = 0
  let answer = result(bool)(i32).err(false) ?? fallback(count)
  if count == 1 { answer } else { 0 }
}

test("coalesce_result_err_fallback.sc") {
  std.test.assert(main() == 42)
}
