let main(): i32 = {
  let base = 39
  let add = { (x: i32)(y: i32)(z: i32) -> base + x + y + z }
  let add_one = add(1)
  let add_two = add_one(1)
  add_two(1)
}

test("closure_partial_multistage.sc") {
  main() == 42
}
