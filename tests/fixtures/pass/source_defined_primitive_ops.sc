let main(): i32 = {
  let mut value = 8
  value += 4
  value -= 2
  value *= 3
  value /= 5
  value %= 4
  value |= 8
  value &= 11
  value ^= 3
  value <<= 1
  value >>= 1

  if !false && true == true && false != true && -value == -9 {
    42
  } else {
    0
  }
}

test("source_defined_primitive_ops.sc") {
  std.test.assert(main() == 42)
}
