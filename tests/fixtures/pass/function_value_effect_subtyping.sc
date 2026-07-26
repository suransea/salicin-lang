let Unsafe = std.unsafe.Unsafe

let pure(): i32 = { 42 }

let invoke(action: (): i32 with(Unsafe))(): i32 with(Unsafe) = { action() }

let main(): i32 = {
  unsafe { invoke(pure)() }
}

test("function_value_effect_subtyping.sc") {
  main() == 42
}
