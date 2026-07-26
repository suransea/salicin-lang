let result = std.result
let throws = std.error.throws

let extract(move result: result(bool)(i32)): i32 with(throws(bool)) = {
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
  main() == 42
}
