let main(): i32 = {
  let base = 40
  let add_base = { (increment: i32) -> base + increment }
  add_base(2)
}

test("capturing_closure.sc") {
  std.test.assert(main() == 42)
}
