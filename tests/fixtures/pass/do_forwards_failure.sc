let result = core.result
let throwing = core.error.throwing

let reject(): i32 with(throwing(bool)) = { throw(true) }

let choose(flag: bool): i32 with(throwing(bool)) = {
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
  main() == 42
}
