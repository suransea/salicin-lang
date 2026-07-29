let unsafety = core.unsafe.unsafety

let increment(value: i32): i32 = { value + 1 }
let dangerous: with(unsafety)(value: i32): i32 = { value + 1 }

let apply(comptime e: effects): with(e)(action: with(e)((i32): i32))(value: i32): i32 = { action(value) }

let main(): i32 = {
  apply(increment)(20) + unsafe {
    apply(dangerous)(20)
  }
}

test("function_value_effect_generic.sc") {
  main() == 42
}
