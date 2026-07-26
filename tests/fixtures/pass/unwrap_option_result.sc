let option = std.option
let result = std.result

let main(): i32 = {
  let option = option.some(20)
  let result = result(e: bool).ok(22)
  option!! + result!!
}

test("unwrap_option_result.sc") {
  main() == 42
}
