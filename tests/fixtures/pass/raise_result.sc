let result = core.result
let throwing = core.error.throwing

let extract: with(throwing(bool))(move result: result(bool)(i32)): i32 = {
  result!
}

let main(): i32 = {
  let success = try {
    extract(result.ok(42))
  }!!
  let failure = try {
    extract(result.err(false))
  } ?? 0
  success + failure
}

test("raise_result.sc") {
  std.test.assert(main() == 42)
}
