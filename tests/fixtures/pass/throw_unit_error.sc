let result = core.result
let throwing = core.error.throwing

let fail: with(throwing(()))(): i32 = {
  throw(())
}

let main(): i32 = {
  let result: result(())(i32) = try { fail() }
  result ?? 42
}

test("throw_unit_error.sc") {
  main() == 42
}
