let result = std.result
let throwing = std.error.throwing

let fail(): i32 with(throwing(bool)) = {
  throw(true)
}

let forward(): i32 with(throwing(bool)) = { fail() }

let main(): i32 = {
  let result: result(bool)(i32) = try { forward() }
  result ?? 42
}

test("throw_result_err_propagate.sc") {
  main() == 42
}
