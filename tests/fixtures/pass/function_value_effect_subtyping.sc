let unsafety = std.unsafe.unsafety

let pure(): i32 = { 42 }

let invoke(action: (): i32 with(unsafety))(): i32 with(unsafety) = { action() }

let main(): i32 = {
  unsafe { invoke(pure)() }
}

test("function_value_effect_subtyping.sc") {
  main() == 42
}
