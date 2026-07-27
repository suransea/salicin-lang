let result = core.result
let throwing = core.error.throwing

let make_error(count: borrow(mut)(i32)): bool = {
  count = count + 1
  true
}

let fail(): i32 with(throwing(bool)) = {
  let mut count = 0
  throw(make_error(count))
}

let main(): i32 = {
  let result: result(bool)(i32) = try { fail() }
  match result
    { ok(_) -> 0 }
    { err(error) -> if error { 42 } else { 0 } }
}

test("throw_error_once.sc") {
  main() == 42
}
