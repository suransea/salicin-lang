let result = core.result
let throwing = core.error.throwing

let choose: with(throwing(bool))(flag: bool): i32 = {
  if flag {
    throw(true)
  } else {
    42
  }
}

let main(): i32 = {
  let first: result(bool)(i32) = try { choose(false) }
  let second: result(bool)(i32) = try { choose(true) }
  (first ?? 0) + (second ?? 0)
}

test("throw_if_flow.sc") {
  main() == 42
}
