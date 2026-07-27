let bit_and = core.ops.bit_and
let bit_or = core.ops.bit_or
let bit_xor = core.ops.bit_xor
let shl = core.ops.shl
let shr = core.ops.shr

let bits = struct { value: i32 }

extend(bits, bit_and(bits)) {
  let output = bits
  let bit_and(self)(rhs: bits): bits = { bits { value: self.value & rhs.value } }
}
extend(bits, bit_or(bits)) {
  let output = bits
  let bit_or(self)(rhs: bits): bits = { bits { value: self.value | rhs.value } }
}
extend(bits, bit_xor(bits)) {
  let output = bits
  let bit_xor(self)(rhs: bits): bits = { bits { value: self.value ^ rhs.value } }
}
extend(bits, shl(bits)) {
  let output = bits
  let shl(self)(rhs: bits): bits = { bits { value: self.value << rhs.value } }
}
extend(bits, shr(bits)) {
  let output = bits
  let shr(self)(rhs: bits): bits = { bits { value: self.value >> rhs.value } }
}

let mask(comptime t: type)(move left: t)(move right: t): t
where t: bit_and(t, output = t) = { left & right }

let unsigned_shift(value: u32): u32 = { value >> 2 }

let main(): i32 = {
  let value = ((((mask(bits { value: 6 })(bits { value: 3 }) | bits { value: 8 }) ^ bits { value: 3 }) << bits { value: 1 }) >> bits { value: 1 }).value
  let builtins = (6 & 3) == 2 && (2 | 8) == 10 && (10 ^ 3) == 9 &&
    (9 << 1) == 18 && (-8 >> 2) == -2 && unsigned_shift(8) == 2
  if value == 9 && builtins { 42 } else { 0 }
}

test("bitwise_operator_traits.sc") {
  main() == 42
}
