let result = std.result
let throws = std.error.throws

let choose(flag: bool): i32 with(throws(bool)) = {
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
