let result = core.result

let main(): i32 = {
  let outer = result(bool)(result(bool)(i32)).err(false)
  let inner = outer ?? result(bool)(i32).ok(42)
  inner ?? 0
}

test("coalesce_nested_result_payload.sc") {
  std.test.assert(main() == 42)
}
