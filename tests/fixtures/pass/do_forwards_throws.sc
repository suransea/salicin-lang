let result = std.result
let throws = std.error.throws

let reject(): i32 with(throws(bool)) = { throw(true) }

let choose(flag: bool): i32 with(throws(bool)) = {
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

test("do_forwards_throws.sc") {
  main() == 42
}
