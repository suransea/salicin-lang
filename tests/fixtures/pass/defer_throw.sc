let Result = std.Result
let Throws = std.error.Throws
let defer = std.control.defer

let fail(counter: borrow(mut)(i32)): i32 with(Throws(bool)) = {
  defer({
    counter = counter + 1
  })
  throw(true)
}

let main(): i32 = {
  let mut counter = 0
  let result: Result(bool)(i32) = try {
    fail(counter)
  }
  match result
    { Ok(_) -> 0 }
    { Err(error) -> if error && counter == 1 { 42 } else { 0 } }
}
