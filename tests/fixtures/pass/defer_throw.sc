let result = std.result
let throws = std.error.throws
let defer = std.control.defer

let fail(counter: borrow(mut)(i32)): i32 with(throws(bool)) = {
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
  main() == 42
}
