let unsafe_effect = std.unsafe.unsafe_effect

let dangerous(): i32 with(unsafe_effect) = { 40 }

let main(): i32 = {
  let action: (i32)(i32): i32 with(unsafe_effect) = {
    (left: i32)(right: i32) -> dangerous() + left + right
  }
  let pending = action(1)
  unsafe {
    pending(1)
  }
}

test("closure_partial_effect.sc") {
  main() == 42
}
