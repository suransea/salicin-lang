let option = std.option
let result = std.result

let main(): i32 = {
  let option_number = option(i32).some(20)
  let option_flag = option(bool).some(true)
  let result_ok = result(bool)(i32).ok(7)
  let result_err = result(i32)(bool).err(5)

  let first = match option_number
    { some(value) -> value }
    { none -> 0 }
  let second = match option_flag
    { some(value) -> if value { 10 } else { 0 } }
    { none -> 0 }
  let third = match result_ok
    { ok(value) -> value }
    { err(_) -> 0 }
  let fourth = match result_err
    { ok(_) -> 0 }
    { err(value) -> value }
  first + second + third + fourth
}

test("core_multiple_instances.sc") {
  main() == 42
}
