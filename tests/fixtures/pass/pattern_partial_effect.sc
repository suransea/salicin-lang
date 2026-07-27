let unsafety = std.unsafe.unsafety

let dangerous(): i32 with(unsafety) = {
  42
}

let main(): i32 = {
  let choose: (bool): core.control.attempt(bool)(i32) with(unsafety) = {
    true -> dangerous()
  }
  let attempted = unsafe {
    choose(true)
  }
  match attempted
    { hit(value) -> value }
    { miss(_) -> 0 }
}

test("pattern_partial_effect.sc") {
  main() == 42
}
