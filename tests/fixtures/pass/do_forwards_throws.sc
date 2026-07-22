use core.effects.Throws

let reject(): i32 with(Throws(bool)) = { throw true }

let choose(flag: bool): i32 with(Throws(bool)) = { do {
  if flag { return reject() }
  42
}
}

let main(): i32 = {
  let success: Result(i32, bool) = try { choose(false) }
  let failure: Result(i32, bool) = try { choose(true) }
  (success ?? 0) + (failure ?? 0)
}
