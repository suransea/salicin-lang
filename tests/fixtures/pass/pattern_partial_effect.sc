let Unsafe = std.unsafe.Unsafe

let dangerous(): i32 with(Unsafe) = {
  42
}

let main(): i32 = {
  let choose: (bool): core.control.Attempt(bool)(i32) with(Unsafe) = {
    true -> dangerous()
  }
  let attempted = unsafe {
    choose(true)
  }
  match attempted
    { Hit(value) -> value }
    { Miss(_) -> 0 }
}

test("pattern_partial_effect.sc") {
  main() == 42
}
