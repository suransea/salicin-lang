let unsafety = core.unsafe.unsafety

let pure(): i32 = { 42 }

let invoke: with(unsafety)(action: with(unsafety)((): i32))(): i32 = { action() }

let main(): i32 = {
  unsafe { invoke(pure)() }
}

test("function_value_effect_subtyping.sc") {
  std.test.assert(main() == 42)
}
