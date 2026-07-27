let result = std.result

let throwing = std.error.throwing
let unsafety = std.unsafe.unsafety

let read(fail: bool): i32 with(throwing(bool), unsafety) = {
  if fail { throw(true) }
  42
}

let main(): i32 = {
  let result: result(bool)(i32) = try {
    unsafe { read(false) }
  }
  result ?? 0
}

test("return_type_effects.sc") {
  main() == 42
}
