let shift(amount: i32): i32 = { 1 << amount }
let main(): i32 = { shift(32) }

test("shift_out_of_range.sc") {
  main() == 42
}
