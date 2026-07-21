use core.ops.{BitAnd, BitOr, BitXor, Shl, Shr}

let Bits = struct(value: i32)

extend Bits: BitAnd(Bits) {
  let Output = Bits
  let bit_and(move self)(move rhs: Bits): Bits = Bits(self.value & rhs.value)
}
extend Bits: BitOr(Bits) {
  let Output = Bits
  let bit_or(move self)(move rhs: Bits): Bits = Bits(self.value | rhs.value)
}
extend Bits: BitXor(Bits) {
  let Output = Bits
  let bit_xor(move self)(move rhs: Bits): Bits = Bits(self.value ^ rhs.value)
}
extend Bits: Shl(Bits) {
  let Output = Bits
  let shl(move self)(move rhs: Bits): Bits = Bits(self.value << rhs.value)
}
extend Bits: Shr(Bits) {
  let Output = Bits
  let shr(move self)(move rhs: Bits): Bits = Bits(self.value >> rhs.value)
}

let mask(T: type)(move left: T)(move right: T): T
where T: BitAnd(T, Output = T) = left & right

let unsigned_shift(value: u32): u32 = value >> 2

let main(): i32 = {
  let value = ((((mask(Bits(6))(Bits(3)) | Bits(8)) ^ Bits(3)) << Bits(1)) >> Bits(1)).value
  let builtins = (6 & 3) == 2 && (2 | 8) == 10 && (10 ^ 3) == 9 &&
    (9 << 1) == 18 && (-8 >> 2) == -2 && unsigned_shift(8) == 2
  if value == 9 && builtins { 42 } else { 0 }
}
