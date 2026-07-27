let result = core.result

let main(): i32 = {
  let value = result(bool)(i32).ok(42)
  match value
    { ok(item) -> item }
    { err(_) -> 0 }
}

test("core_result_ok.sc") {
  main() == 42
}
