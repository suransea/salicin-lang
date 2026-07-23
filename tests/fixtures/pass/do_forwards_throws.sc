use std.Result
use std.effect.Throws

let reject(): i32 with(Throws(bool)) = { throw(true) }

let choose(flag: bool): i32 with(Throws(bool)) = { do {
  if flag { return reject() }
  42
}
}

let main(): i32 = {
  let success: Result(bool)(i32) = try { choose(false) }
  let failure: Result(bool)(i32) = try { choose(true) }
  (success ?? 0) + (failure ?? 0)
}
