let result = core.result
let throwing = core.error.throwing

let read(fail: bool): i32 with(throwing(bool)) = {
  if fail { throw(true) } else { 40 }
}

let main(): i32 = {
  let propagated: result(bool)(i32) = try {
    read(true) + 2
  }
  let thrown: result(bool)(i32) = try {
    throw(true)
  }
  let success: result(bool)(i32) = try {
    read(false) + 2
  }
  let propagation_ok = match propagated
    { ok(_) -> false }
    { err(error) -> error }
  let throw_ok = match thrown
    { ok(_) -> false }
    { err(error) -> error }
  let value = match success
    { ok(value) -> value }
    { err(_) -> 0 }
  if propagation_ok && throw_ok && value == 42 { 42 } else { 0 }
}

test("do_try_boundary.sc") {
  main() == 42
}
