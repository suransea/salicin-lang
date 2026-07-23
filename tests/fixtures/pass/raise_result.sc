use std.Result
use std.effect.Throws

let extract(move result: Result(bool)(i32)): i32 with(Throws(bool)) = {
  result!
}

let main(): i32 = {
  let success = try {
    extract(Result.Ok(42))
  }!!
  let failure = try {
    extract(Result.Err(false))
  } ?? 0
  success + failure
}
