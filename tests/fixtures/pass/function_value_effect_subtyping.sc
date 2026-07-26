let unsafe_effect = std.unsafe.unsafe_effect

let pure(): i32 = { 42 }

let invoke(action: (): i32 with(unsafe_effect))(): i32 with(unsafe_effect) = { action() }

let main(): i32 = {
  unsafe { invoke(pure)() }
}

test("function_value_effect_subtyping.sc") {
  main() == 42
}
