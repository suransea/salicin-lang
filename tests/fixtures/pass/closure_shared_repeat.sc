let main(): i32 = {
  let base = 20
  let add_base = { (increment: i32) -> base + increment }
  add_base(1) + add_base(1)
}

test("closure_shared_repeat.sc") {
  std.test.assert(main() == 42)
}
