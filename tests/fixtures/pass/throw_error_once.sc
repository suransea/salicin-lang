let Result = std.Result
let Throws = std.effect.Throws

let make_error(count: borrow(mut)(i32)): bool = {
  count = count + 1
  true
}

let fail(): i32 with(Throws(bool)) = {
  let mut count = 0
  throw(make_error(count))
}

let main(): i32 = {
  let result: Result(bool)(i32) = try { fail() }
  match result
    { Ok(_) -> 0 }
    { Err(error) -> if error { 42 } else { 0 } }
}
