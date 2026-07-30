let result = core.result
let throwing = core.error.throwing

let reject: with(throwing(bool))(): i32 = { throw(true) }

let choose: with(throwing(bool))(flag: bool): i32 = {
  do {
    if flag { return(reject()) }
    42
  }
}

let main(): i32 = {
  let success: result(bool)(i32) = try { choose(false) }
  let failure: result(bool)(i32) = try { choose(true) }
  (success ?? 0) + (failure ?? 0)
}

test("do_forwards_failure.sc") {
  std.test.assert(main() == 42)
}
