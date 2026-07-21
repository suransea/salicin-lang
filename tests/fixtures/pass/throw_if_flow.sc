let choose(flag: bool): i32 with(throws(bool)) = {
  if flag {
    throw true
  } else {
    42
  }
}

let main(): i32 = {
  let first: Result(i32, bool) = try { choose(false) }
  let second: Result(i32, bool) = try { choose(true) }
  (first ?? 0) + (second ?? 0)
}
