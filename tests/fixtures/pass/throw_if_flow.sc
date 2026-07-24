let Result = std.Result
let Throws = std.effect.Throws

let choose(flag: bool): i32 with(Throws(bool)) = {
  if flag {
    throw(true)
  } else {
    42
  }
}

let main(): i32 = {
  let first: Result(bool)(i32) = try { choose(false) }
  let second: Result(bool)(i32) = try { choose(true) }
  (first ?? 0) + (second ?? 0)
}
