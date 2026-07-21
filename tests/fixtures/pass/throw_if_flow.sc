let choose(flag: bool): Result(i32, bool) = {
  if flag {
    throw true
  } else {
    42
  }
}

let main(): i32 = (choose(false) ?? 0) + (choose(true) ?? 0)
