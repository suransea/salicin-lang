let shift(amount: i32): i32 = { 1 << amount }
let main(): i32 = { shift(-1) }

test("shift_negative.sc") {
  std.test.assert(main() == 42)
}
