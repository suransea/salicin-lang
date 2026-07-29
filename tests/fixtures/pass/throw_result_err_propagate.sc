let result = core.result
let throwing = core.error.throwing

let fail: with(throwing(bool))(): i32 = {
  throw(true)
}

let forward: with(throwing(bool))(): i32 = { fail() }

let main(): i32 = {
  let result: result(bool)(i32) = try { forward() }
  result ?? 42
}

test("throw_result_err_propagate.sc") {
  main() == 42
}
