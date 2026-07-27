let unsafety = core.unsafe.unsafety

let increment(value: i32): i32 = { value + 1 }
let dangerous(value: i32): i32 with(unsafety) = { value + 1 }

let apply(comptime e: effects)
  (action: (i32): i32 with(e))
  (value: i32): i32 with(e) = { action(value) }

let main(): i32 = {
  apply(increment)(20) + unsafe {
    apply(dangerous)(20)
  }
}

test("function_value_effect_generic.sc") {
  main() == 42
}
