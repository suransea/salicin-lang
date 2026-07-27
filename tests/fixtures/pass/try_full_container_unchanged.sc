let option = core.option
let result = core.result

let option_tail(): option(i32) = { option(i32).some(10) }

let option_return(): option(i32) = {
  return(option(i32).some(10))
}

let result_tail(): result(bool)(i32) = { result(bool)(i32).ok(11) }

let result_return(): result(bool)(i32) = {
  return(result(bool)(i32).ok(11))
}

let main(): i32 = {
  let first = option_tail() ?? 0
  let second = option_return() ?? 0
  let third = result_tail() ?? 0
  let fourth = result_return() ?? 0
  first + second + third + fourth
}

test("try_full_container_unchanged.sc") {
  main() == 42
}
