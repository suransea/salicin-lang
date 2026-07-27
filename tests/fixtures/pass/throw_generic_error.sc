let result = std.result
let throwing = std.error.throwing

let fail(comptime e: type)(move error: e): i32 with(throwing(e)) = {
  throw(error)
}

let main(): i32 = {
  let result: result(bool)(i32) = try { fail(bool)(true) }
  result ?? 42
}

test("throw_generic_error.sc") {
  main() == 42
}
