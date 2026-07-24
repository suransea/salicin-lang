let BitAnd = std.ops.BitAnd
let BitOr = std.ops.BitOr
let BitXor = std.ops.BitXor
let Shl = std.ops.Shl
let Shr = std.ops.Shr

let Bits = struct { value: i32 }

extend Bits: BitAnd(Bits) {
  let Output = Bits
  let bit_and(self)(rhs: Bits): Bits = { Bits { value: self.value & rhs.value } }
}
extend Bits: BitOr(Bits) {
  let Output = Bits
  let bit_or(self)(rhs: Bits): Bits = { Bits { value: self.value | rhs.value } }
}
extend Bits: BitXor(Bits) {
  let Output = Bits
  let bit_xor(self)(rhs: Bits): Bits = { Bits { value: self.value ^ rhs.value } }
}
extend Bits: Shl(Bits) {
  let Output = Bits
  let shl(self)(rhs: Bits): Bits = { Bits { value: self.value << rhs.value } }
}
extend Bits: Shr(Bits) {
  let Output = Bits
  let shr(self)(rhs: Bits): Bits = { Bits { value: self.value >> rhs.value } }
}

let mask(T: type)(move left: T)(move right: T): T
where T: BitAnd(T, Output = T) = { left & right }

let unsigned_shift(value: u32): u32 = { value >> 2 }

let main(): i32 = {
  let value = ((((mask(Bits { value: 6 })(Bits { value: 3 }) | Bits { value: 8 }) ^ Bits { value: 3 }) << Bits { value: 1 }) >> Bits { value: 1 }).value
  let builtins = (6 & 3) == 2 && (2 | 8) == 10 && (10 ^ 3) == 9 &&
    (9 << 1) == 18 && (-8 >> 2) == -2 && unsigned_shift(8) == 2
  if value == 9 && builtins { 42 } else { 0 }
}
