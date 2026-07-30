let main(): i32 = {
  let base = 40
  let add = { (x: i32)(y: i32) -> base + x + y }
  let add_one = add(1)
  add_one(1)
}

test("closure_partial_application.sc") {
  std.test.assert(main() == 42)
}
