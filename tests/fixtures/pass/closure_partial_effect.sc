let unsafety = core.unsafe.unsafety

let dangerous: with(unsafety)(): i32 = { 40 }

let main(): i32 = {
  let action: with(unsafety)((i32)(i32): i32)  = {
    (left: i32)(right: i32) -> dangerous() + left + right
  }
  let pending = action(1)
  unsafe {
    pending(1)
  }
}

test("closure_partial_effect.sc") {
  std.test.assert(main() == 42)
}
