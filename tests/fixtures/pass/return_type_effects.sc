let result = core.result

let throwing = core.error.throwing
let unsafety = core.unsafe.unsafety

let read: with(throwing(bool), unsafety)(fail: bool): i32 = {
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
  std.test.assert(main() == 42)
}
