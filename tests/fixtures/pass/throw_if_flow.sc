use core.Result
use core.effects.Throws

let choose(flag: bool): i32 with(Throws(bool)) = {
  if flag {
    throw(true)
  } else {
    42
  }
}

let main(): i32 = {
  let first: Result(i32, bool) = try { choose(false) }
  let second: Result(i32, bool) = try { choose(true) }
  (first ?? 0) + (second ?? 0)
}
