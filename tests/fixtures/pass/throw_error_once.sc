let make_error(borrow(mut) count: i32): bool = {
  count = count + 1
  true
}

let fail(): i32 with(throws(bool)) = {
  let mut count = 0
  throw make_error(count)
}

let main(): i32 = {
  let result: Result(i32, bool) = try { fail() }
  result match { Ok(_) => 0, Err(error) => if error { 42 } else { 0 } }
}
