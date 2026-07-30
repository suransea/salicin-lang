let consume(move value: i32): i32 = { value }

let main(): i32 = {
  let value = 42
  consume(value)
}

test("explicit_move_i32_once.sc") {
  std.test.assert(main() == 42)
}
