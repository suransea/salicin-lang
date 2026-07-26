let option = std.option
let result = std.result

let main(): i32 = {
  let inner = result(bool)(i32).ok(42)
  let outer = option(result(bool)(i32)).some(inner)
  match outer
    { some(result) -> match result
      { ok(value) -> value }
      { err(_) -> 0 } }
    { none -> 0 }
}

test("core_nested_option_result.sc") {
  main() == 42
}
