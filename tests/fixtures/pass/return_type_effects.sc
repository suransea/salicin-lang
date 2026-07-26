let result = std.result

let throws = std.error.throws
let unsafe_effect = std.unsafe.unsafe_effect

let read(fail: bool): i32 with(throws(bool), unsafe_effect) = {
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
