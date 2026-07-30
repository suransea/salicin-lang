let result = core.result
let throwing = core.error.throwing

let fail(comptime e: type): with(throwing(e))(move error: e): i32 = {
  throw(error)
}

let main(): i32 = {
  let result: result(bool)(i32) = try { fail(bool)(true) }
  result ?? 42
}

test("throw_generic_error.sc") {
  std.test.assert(main() == 42)
}
