let result = core.result
let throwing = core.error.throwing
let defer = core.control.defer

let fail: with(throwing(bool))(counter: borrow(mut)(i32)): i32 = {
  defer {
      counter = counter + 1
    }
  throw(true)
}

let main(): i32 = {
  let mut counter = 0
  let result: result(bool)(i32) = try {
    fail(counter)
  }
  match result
    { ok(_) -> 0 }
    { err(error) -> if error && counter == 1 { 42 } else { 0 } }
}

test("defer_throw.sc") {
  std.test.assert(main() == 42)
}
