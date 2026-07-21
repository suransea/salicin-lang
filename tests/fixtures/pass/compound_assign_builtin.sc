let Cell = struct(value: i32)

let main(): i32 = {
  let mut add = Cell(40)
  add.value += 2

  let mut sub = 44
  sub -= 2

  let mut mul = 21
  mul *= 2

  let mut div = 84
  div /= 2

  let mut rem = 85
  rem %= 43

  let mut array = [40, 0]
  array[0] += 2

  let mut bit_and = 47
  bit_and &= 42

  let mut bit_or = 40
  bit_or |= 2

  let mut bit_xor = 40
  bit_xor ^= 2

  let mut shl = 21
  shl <<= 1

  let mut shr = 84
  shr >>= 1

  (add.value + sub + mul + div + rem + array[0] + bit_and + bit_or + bit_xor + shl + shr) / 11
}
