// Opaque primitive types. Their source identities and contracts participate in
// ordinary type and trait resolution; the compiler only supplies representation
// and intrinsic lowering after validating this core bundle.
pub let bool = enum { false, true }
pub let i8: type = builtin()
pub let i16: type = builtin()
pub let i32: type = builtin()
pub let i64: type = builtin()
pub let i128: type = builtin()
pub let isize: type = builtin()
pub let u8: type = builtin()
pub let u16: type = builtin()
pub let u32: type = builtin()
pub let u64: type = builtin()
pub let u128: type = builtin()
pub let usize: type = builtin()

extend bool: core.marker.copyable {}
extend bool: core.ops.bit.not {
  let output = bool
  let not(self)(): bool = {
    match self
      { false -> true }
      { true -> false }
  }
}
extend bool: core.cmp.eq(bool) {
  let eq(self: borrow(bool))(rhs: borrow(bool)): bool = {
    match self
      { false -> match rhs
        { false -> true }
        { true -> false } }
      { true -> match rhs
        { false -> false }
        { true -> true } }
  }
}

// Integer contracts are deliberately stated in source. Only irreducible
// scalar operations use `builtin()`; derived negation and compound assignment
// are ordinary Salicin definitions.
extend i8: core.marker.copyable {}
extend i8: core.ops.arith.add(i8) { let output = i8; let add(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.arith.sub(i8) { let output = i8; let sub(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.arith.mul(i8) { let output = i8; let mul(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.arith.div(i8) { let output = i8; let div(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.arith.rem(i8) { let output = i8; let rem(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.arith.neg { let output = i8; let neg(self)(): i8 = { 0 - self } }
extend i8: core.ops.bit.bit_and(i8) { let output = i8; let bit_and(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.bit.bit_or(i8) { let output = i8; let bit_or(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.bit.bit_xor(i8) { let output = i8; let bit_xor(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.bit.shl(i8) { let output = i8; let shl(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.bit.shr(i8) { let output = i8; let shr(self)(rhs: i8): i8 = builtin() }
extend i8: core.cmp.eq(i8) { let eq(self: borrow(i8))(rhs: borrow(i8)): bool = builtin() }
extend i8: core.cmp.partial_ord(i8) { let partial_cmp(self: borrow(i8))(rhs: borrow(i8)): core.cmp.partial_ordering = builtin() }
extend i8: core.ops.assign.add_assign(i8) { let add_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self + rhs } }
extend i8: core.ops.assign.sub_assign(i8) { let sub_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self - rhs } }
extend i8: core.ops.assign.mul_assign(i8) { let mul_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self * rhs } }
extend i8: core.ops.assign.div_assign(i8) { let div_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self / rhs } }
extend i8: core.ops.assign.rem_assign(i8) { let rem_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self % rhs } }
extend i8: core.ops.assign.bit_and_assign(i8) { let bit_and_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self & rhs } }
extend i8: core.ops.assign.bit_or_assign(i8) { let bit_or_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self | rhs } }
extend i8: core.ops.assign.bit_xor_assign(i8) { let bit_xor_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self ^ rhs } }
extend i8: core.ops.assign.shl_assign(i8) { let shl_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self << rhs } }
extend i8: core.ops.assign.shr_assign(i8) { let shr_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self >> rhs } }

extend i16: core.marker.copyable {}
extend i16: core.ops.arith.add(i16) { let output = i16; let add(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.arith.sub(i16) { let output = i16; let sub(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.arith.mul(i16) { let output = i16; let mul(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.arith.div(i16) { let output = i16; let div(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.arith.rem(i16) { let output = i16; let rem(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.arith.neg { let output = i16; let neg(self)(): i16 = { 0 - self } }
extend i16: core.ops.bit.bit_and(i16) { let output = i16; let bit_and(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.bit.bit_or(i16) { let output = i16; let bit_or(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.bit.bit_xor(i16) { let output = i16; let bit_xor(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.bit.shl(i16) { let output = i16; let shl(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.bit.shr(i16) { let output = i16; let shr(self)(rhs: i16): i16 = builtin() }
extend i16: core.cmp.eq(i16) { let eq(self: borrow(i16))(rhs: borrow(i16)): bool = builtin() }
extend i16: core.cmp.partial_ord(i16) { let partial_cmp(self: borrow(i16))(rhs: borrow(i16)): core.cmp.partial_ordering = builtin() }
extend i16: core.ops.assign.add_assign(i16) { let add_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self + rhs } }
extend i16: core.ops.assign.sub_assign(i16) { let sub_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self - rhs } }
extend i16: core.ops.assign.mul_assign(i16) { let mul_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self * rhs } }
extend i16: core.ops.assign.div_assign(i16) { let div_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self / rhs } }
extend i16: core.ops.assign.rem_assign(i16) { let rem_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self % rhs } }
extend i16: core.ops.assign.bit_and_assign(i16) { let bit_and_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self & rhs } }
extend i16: core.ops.assign.bit_or_assign(i16) { let bit_or_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self | rhs } }
extend i16: core.ops.assign.bit_xor_assign(i16) { let bit_xor_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self ^ rhs } }
extend i16: core.ops.assign.shl_assign(i16) { let shl_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self << rhs } }
extend i16: core.ops.assign.shr_assign(i16) { let shr_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self >> rhs } }

extend i128: core.marker.copyable {}
extend i128: core.ops.arith.add(i128) { let output = i128; let add(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.arith.sub(i128) { let output = i128; let sub(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.arith.mul(i128) { let output = i128; let mul(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.arith.div(i128) { let output = i128; let div(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.arith.rem(i128) { let output = i128; let rem(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.arith.neg { let output = i128; let neg(self)(): i128 = { 0 - self } }
extend i128: core.ops.bit.bit_and(i128) { let output = i128; let bit_and(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.bit.bit_or(i128) { let output = i128; let bit_or(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.bit.bit_xor(i128) { let output = i128; let bit_xor(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.bit.shl(i128) { let output = i128; let shl(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.bit.shr(i128) { let output = i128; let shr(self)(rhs: i128): i128 = builtin() }
extend i128: core.cmp.eq(i128) { let eq(self: borrow(i128))(rhs: borrow(i128)): bool = builtin() }
extend i128: core.cmp.partial_ord(i128) { let partial_cmp(self: borrow(i128))(rhs: borrow(i128)): core.cmp.partial_ordering = builtin() }
extend i128: core.ops.assign.add_assign(i128) { let add_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self + rhs } }
extend i128: core.ops.assign.sub_assign(i128) { let sub_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self - rhs } }
extend i128: core.ops.assign.mul_assign(i128) { let mul_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self * rhs } }
extend i128: core.ops.assign.div_assign(i128) { let div_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self / rhs } }
extend i128: core.ops.assign.rem_assign(i128) { let rem_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self % rhs } }
extend i128: core.ops.assign.bit_and_assign(i128) { let bit_and_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self & rhs } }
extend i128: core.ops.assign.bit_or_assign(i128) { let bit_or_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self | rhs } }
extend i128: core.ops.assign.bit_xor_assign(i128) { let bit_xor_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self ^ rhs } }
extend i128: core.ops.assign.shl_assign(i128) { let shl_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self << rhs } }
extend i128: core.ops.assign.shr_assign(i128) { let shr_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self >> rhs } }

extend isize: core.marker.copyable {}
extend isize: core.ops.arith.add(isize) { let output = isize; let add(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.arith.sub(isize) { let output = isize; let sub(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.arith.mul(isize) { let output = isize; let mul(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.arith.div(isize) { let output = isize; let div(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.arith.rem(isize) { let output = isize; let rem(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.arith.neg { let output = isize; let neg(self)(): isize = { 0 - self } }
extend isize: core.ops.bit.bit_and(isize) { let output = isize; let bit_and(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.bit.bit_or(isize) { let output = isize; let bit_or(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.bit.bit_xor(isize) { let output = isize; let bit_xor(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.bit.shl(isize) { let output = isize; let shl(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.bit.shr(isize) { let output = isize; let shr(self)(rhs: isize): isize = builtin() }
extend isize: core.cmp.eq(isize) { let eq(self: borrow(isize))(rhs: borrow(isize)): bool = builtin() }
extend isize: core.cmp.partial_ord(isize) { let partial_cmp(self: borrow(isize))(rhs: borrow(isize)): core.cmp.partial_ordering = builtin() }
extend isize: core.ops.assign.add_assign(isize) { let add_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self + rhs } }
extend isize: core.ops.assign.sub_assign(isize) { let sub_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self - rhs } }
extend isize: core.ops.assign.mul_assign(isize) { let mul_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self * rhs } }
extend isize: core.ops.assign.div_assign(isize) { let div_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self / rhs } }
extend isize: core.ops.assign.rem_assign(isize) { let rem_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self % rhs } }
extend isize: core.ops.assign.bit_and_assign(isize) { let bit_and_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self & rhs } }
extend isize: core.ops.assign.bit_or_assign(isize) { let bit_or_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self | rhs } }
extend isize: core.ops.assign.bit_xor_assign(isize) { let bit_xor_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self ^ rhs } }
extend isize: core.ops.assign.shl_assign(isize) { let shl_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self << rhs } }
extend isize: core.ops.assign.shr_assign(isize) { let shr_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self >> rhs } }

extend u8: core.marker.copyable {}
extend u8: core.ops.arith.add(u8) { let output = u8; let add(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.arith.sub(u8) { let output = u8; let sub(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.arith.mul(u8) { let output = u8; let mul(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.arith.div(u8) { let output = u8; let div(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.arith.rem(u8) { let output = u8; let rem(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.bit.bit_and(u8) { let output = u8; let bit_and(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.bit.bit_or(u8) { let output = u8; let bit_or(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.bit.bit_xor(u8) { let output = u8; let bit_xor(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.bit.shl(u8) { let output = u8; let shl(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.bit.shr(u8) { let output = u8; let shr(self)(rhs: u8): u8 = builtin() }
extend u8: core.cmp.eq(u8) { let eq(self: borrow(u8))(rhs: borrow(u8)): bool = builtin() }
extend u8: core.cmp.partial_ord(u8) { let partial_cmp(self: borrow(u8))(rhs: borrow(u8)): core.cmp.partial_ordering = builtin() }
extend u8: core.ops.assign.add_assign(u8) { let add_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self + rhs } }
extend u8: core.ops.assign.sub_assign(u8) { let sub_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self - rhs } }
extend u8: core.ops.assign.mul_assign(u8) { let mul_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self * rhs } }
extend u8: core.ops.assign.div_assign(u8) { let div_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self / rhs } }
extend u8: core.ops.assign.rem_assign(u8) { let rem_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self % rhs } }
extend u8: core.ops.assign.bit_and_assign(u8) { let bit_and_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self & rhs } }
extend u8: core.ops.assign.bit_or_assign(u8) { let bit_or_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self | rhs } }
extend u8: core.ops.assign.bit_xor_assign(u8) { let bit_xor_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self ^ rhs } }
extend u8: core.ops.assign.shl_assign(u8) { let shl_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self << rhs } }
extend u8: core.ops.assign.shr_assign(u8) { let shr_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self >> rhs } }

extend u16: core.marker.copyable {}
extend u16: core.ops.arith.add(u16) { let output = u16; let add(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.arith.sub(u16) { let output = u16; let sub(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.arith.mul(u16) { let output = u16; let mul(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.arith.div(u16) { let output = u16; let div(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.arith.rem(u16) { let output = u16; let rem(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.bit.bit_and(u16) { let output = u16; let bit_and(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.bit.bit_or(u16) { let output = u16; let bit_or(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.bit.bit_xor(u16) { let output = u16; let bit_xor(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.bit.shl(u16) { let output = u16; let shl(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.bit.shr(u16) { let output = u16; let shr(self)(rhs: u16): u16 = builtin() }
extend u16: core.cmp.eq(u16) { let eq(self: borrow(u16))(rhs: borrow(u16)): bool = builtin() }
extend u16: core.cmp.partial_ord(u16) { let partial_cmp(self: borrow(u16))(rhs: borrow(u16)): core.cmp.partial_ordering = builtin() }
extend u16: core.ops.assign.add_assign(u16) { let add_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self + rhs } }
extend u16: core.ops.assign.sub_assign(u16) { let sub_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self - rhs } }
extend u16: core.ops.assign.mul_assign(u16) { let mul_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self * rhs } }
extend u16: core.ops.assign.div_assign(u16) { let div_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self / rhs } }
extend u16: core.ops.assign.rem_assign(u16) { let rem_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self % rhs } }
extend u16: core.ops.assign.bit_and_assign(u16) { let bit_and_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self & rhs } }
extend u16: core.ops.assign.bit_or_assign(u16) { let bit_or_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self | rhs } }
extend u16: core.ops.assign.bit_xor_assign(u16) { let bit_xor_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self ^ rhs } }
extend u16: core.ops.assign.shl_assign(u16) { let shl_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self << rhs } }
extend u16: core.ops.assign.shr_assign(u16) { let shr_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self >> rhs } }

extend u128: core.marker.copyable {}
extend u128: core.ops.arith.add(u128) { let output = u128; let add(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.arith.sub(u128) { let output = u128; let sub(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.arith.mul(u128) { let output = u128; let mul(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.arith.div(u128) { let output = u128; let div(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.arith.rem(u128) { let output = u128; let rem(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.bit.bit_and(u128) { let output = u128; let bit_and(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.bit.bit_or(u128) { let output = u128; let bit_or(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.bit.bit_xor(u128) { let output = u128; let bit_xor(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.bit.shl(u128) { let output = u128; let shl(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.bit.shr(u128) { let output = u128; let shr(self)(rhs: u128): u128 = builtin() }
extend u128: core.cmp.eq(u128) { let eq(self: borrow(u128))(rhs: borrow(u128)): bool = builtin() }
extend u128: core.cmp.partial_ord(u128) { let partial_cmp(self: borrow(u128))(rhs: borrow(u128)): core.cmp.partial_ordering = builtin() }
extend u128: core.ops.assign.add_assign(u128) { let add_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self + rhs } }
extend u128: core.ops.assign.sub_assign(u128) { let sub_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self - rhs } }
extend u128: core.ops.assign.mul_assign(u128) { let mul_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self * rhs } }
extend u128: core.ops.assign.div_assign(u128) { let div_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self / rhs } }
extend u128: core.ops.assign.rem_assign(u128) { let rem_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self % rhs } }
extend u128: core.ops.assign.bit_and_assign(u128) { let bit_and_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self & rhs } }
extend u128: core.ops.assign.bit_or_assign(u128) { let bit_or_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self | rhs } }
extend u128: core.ops.assign.bit_xor_assign(u128) { let bit_xor_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self ^ rhs } }
extend u128: core.ops.assign.shl_assign(u128) { let shl_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self << rhs } }
extend u128: core.ops.assign.shr_assign(u128) { let shr_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self >> rhs } }

extend usize: core.marker.copyable {}
extend usize: core.ops.arith.add(usize) { let output = usize; let add(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.arith.sub(usize) { let output = usize; let sub(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.arith.mul(usize) { let output = usize; let mul(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.arith.div(usize) { let output = usize; let div(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.arith.rem(usize) { let output = usize; let rem(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.bit.bit_and(usize) { let output = usize; let bit_and(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.bit.bit_or(usize) { let output = usize; let bit_or(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.bit.bit_xor(usize) { let output = usize; let bit_xor(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.bit.shl(usize) { let output = usize; let shl(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.bit.shr(usize) { let output = usize; let shr(self)(rhs: usize): usize = builtin() }
extend usize: core.cmp.eq(usize) { let eq(self: borrow(usize))(rhs: borrow(usize)): bool = builtin() }
extend usize: core.cmp.partial_ord(usize) { let partial_cmp(self: borrow(usize))(rhs: borrow(usize)): core.cmp.partial_ordering = builtin() }
extend usize: core.ops.assign.add_assign(usize) { let add_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self + rhs } }
extend usize: core.ops.assign.sub_assign(usize) { let sub_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self - rhs } }
extend usize: core.ops.assign.mul_assign(usize) { let mul_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self * rhs } }
extend usize: core.ops.assign.div_assign(usize) { let div_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self / rhs } }
extend usize: core.ops.assign.rem_assign(usize) { let rem_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self % rhs } }
extend usize: core.ops.assign.bit_and_assign(usize) { let bit_and_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self & rhs } }
extend usize: core.ops.assign.bit_or_assign(usize) { let bit_or_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self | rhs } }
extend usize: core.ops.assign.bit_xor_assign(usize) { let bit_xor_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self ^ rhs } }
extend usize: core.ops.assign.shl_assign(usize) { let shl_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self << rhs } }
extend usize: core.ops.assign.shr_assign(usize) { let shr_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self >> rhs } }

extend i32: core.marker.copyable {}
extend i32: core.ops.arith.add(i32) { let output = i32; let add(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.arith.sub(i32) { let output = i32; let sub(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.arith.mul(i32) { let output = i32; let mul(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.arith.div(i32) { let output = i32; let div(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.arith.rem(i32) { let output = i32; let rem(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.arith.neg { let output = i32; let neg(self)(): i32 = { 0 - self } }
extend i32: core.ops.bit.bit_and(i32) { let output = i32; let bit_and(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.bit.bit_or(i32) { let output = i32; let bit_or(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.bit.bit_xor(i32) { let output = i32; let bit_xor(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.bit.shl(i32) { let output = i32; let shl(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.bit.shr(i32) { let output = i32; let shr(self)(rhs: i32): i32 = builtin() }
extend i32: core.cmp.eq(i32) { let eq(self: borrow(i32))(rhs: borrow(i32)): bool = builtin() }
extend i32: core.cmp.partial_ord(i32) { let partial_cmp(self: borrow(i32))(rhs: borrow(i32)): core.cmp.partial_ordering = builtin() }
extend i32: core.ops.assign.add_assign(i32) { let add_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self + rhs } }
extend i32: core.ops.assign.sub_assign(i32) { let sub_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self - rhs } }
extend i32: core.ops.assign.mul_assign(i32) { let mul_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self * rhs } }
extend i32: core.ops.assign.div_assign(i32) { let div_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self / rhs } }
extend i32: core.ops.assign.rem_assign(i32) { let rem_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self % rhs } }
extend i32: core.ops.assign.bit_and_assign(i32) { let bit_and_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self & rhs } }
extend i32: core.ops.assign.bit_or_assign(i32) { let bit_or_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self | rhs } }
extend i32: core.ops.assign.bit_xor_assign(i32) { let bit_xor_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self ^ rhs } }
extend i32: core.ops.assign.shl_assign(i32) { let shl_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self << rhs } }
extend i32: core.ops.assign.shr_assign(i32) { let shr_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self >> rhs } }

extend i64: core.marker.copyable {}
extend i64: core.ops.arith.add(i64) { let output = i64; let add(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.arith.sub(i64) { let output = i64; let sub(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.arith.mul(i64) { let output = i64; let mul(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.arith.div(i64) { let output = i64; let div(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.arith.rem(i64) { let output = i64; let rem(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.arith.neg { let output = i64; let neg(self)(): i64 = { 0 - self } }
extend i64: core.ops.bit.bit_and(i64) { let output = i64; let bit_and(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.bit.bit_or(i64) { let output = i64; let bit_or(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.bit.bit_xor(i64) { let output = i64; let bit_xor(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.bit.shl(i64) { let output = i64; let shl(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.bit.shr(i64) { let output = i64; let shr(self)(rhs: i64): i64 = builtin() }
extend i64: core.cmp.eq(i64) { let eq(self: borrow(i64))(rhs: borrow(i64)): bool = builtin() }
extend i64: core.cmp.partial_ord(i64) { let partial_cmp(self: borrow(i64))(rhs: borrow(i64)): core.cmp.partial_ordering = builtin() }
extend i64: core.ops.assign.add_assign(i64) { let add_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self + rhs } }
extend i64: core.ops.assign.sub_assign(i64) { let sub_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self - rhs } }
extend i64: core.ops.assign.mul_assign(i64) { let mul_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self * rhs } }
extend i64: core.ops.assign.div_assign(i64) { let div_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self / rhs } }
extend i64: core.ops.assign.rem_assign(i64) { let rem_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self % rhs } }
extend i64: core.ops.assign.bit_and_assign(i64) { let bit_and_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self & rhs } }
extend i64: core.ops.assign.bit_or_assign(i64) { let bit_or_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self | rhs } }
extend i64: core.ops.assign.bit_xor_assign(i64) { let bit_xor_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self ^ rhs } }
extend i64: core.ops.assign.shl_assign(i64) { let shl_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self << rhs } }
extend i64: core.ops.assign.shr_assign(i64) { let shr_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self >> rhs } }

extend u32: core.marker.copyable {}
extend u32: core.ops.arith.add(u32) { let output = u32; let add(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.arith.sub(u32) { let output = u32; let sub(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.arith.mul(u32) { let output = u32; let mul(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.arith.div(u32) { let output = u32; let div(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.arith.rem(u32) { let output = u32; let rem(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.bit.bit_and(u32) { let output = u32; let bit_and(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.bit.bit_or(u32) { let output = u32; let bit_or(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.bit.bit_xor(u32) { let output = u32; let bit_xor(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.bit.shl(u32) { let output = u32; let shl(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.bit.shr(u32) { let output = u32; let shr(self)(rhs: u32): u32 = builtin() }
extend u32: core.cmp.eq(u32) { let eq(self: borrow(u32))(rhs: borrow(u32)): bool = builtin() }
extend u32: core.cmp.partial_ord(u32) { let partial_cmp(self: borrow(u32))(rhs: borrow(u32)): core.cmp.partial_ordering = builtin() }
extend u32: core.ops.assign.add_assign(u32) { let add_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self + rhs } }
extend u32: core.ops.assign.sub_assign(u32) { let sub_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self - rhs } }
extend u32: core.ops.assign.mul_assign(u32) { let mul_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self * rhs } }
extend u32: core.ops.assign.div_assign(u32) { let div_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self / rhs } }
extend u32: core.ops.assign.rem_assign(u32) { let rem_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self % rhs } }
extend u32: core.ops.assign.bit_and_assign(u32) { let bit_and_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self & rhs } }
extend u32: core.ops.assign.bit_or_assign(u32) { let bit_or_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self | rhs } }
extend u32: core.ops.assign.bit_xor_assign(u32) { let bit_xor_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self ^ rhs } }
extend u32: core.ops.assign.shl_assign(u32) { let shl_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self << rhs } }
extend u32: core.ops.assign.shr_assign(u32) { let shr_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self >> rhs } }

extend u64: core.marker.copyable {}
extend u64: core.ops.arith.add(u64) { let output = u64; let add(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.arith.sub(u64) { let output = u64; let sub(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.arith.mul(u64) { let output = u64; let mul(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.arith.div(u64) { let output = u64; let div(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.arith.rem(u64) { let output = u64; let rem(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.bit.bit_and(u64) { let output = u64; let bit_and(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.bit.bit_or(u64) { let output = u64; let bit_or(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.bit.bit_xor(u64) { let output = u64; let bit_xor(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.bit.shl(u64) { let output = u64; let shl(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.bit.shr(u64) { let output = u64; let shr(self)(rhs: u64): u64 = builtin() }
extend u64: core.cmp.eq(u64) { let eq(self: borrow(u64))(rhs: borrow(u64)): bool = builtin() }
extend u64: core.cmp.partial_ord(u64) { let partial_cmp(self: borrow(u64))(rhs: borrow(u64)): core.cmp.partial_ordering = builtin() }
extend u64: core.ops.assign.add_assign(u64) { let add_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self + rhs } }
extend u64: core.ops.assign.sub_assign(u64) { let sub_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self - rhs } }
extend u64: core.ops.assign.mul_assign(u64) { let mul_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self * rhs } }
extend u64: core.ops.assign.div_assign(u64) { let div_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self / rhs } }
extend u64: core.ops.assign.rem_assign(u64) { let rem_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self % rhs } }
extend u64: core.ops.assign.bit_and_assign(u64) { let bit_and_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self & rhs } }
extend u64: core.ops.assign.bit_or_assign(u64) { let bit_or_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self | rhs } }
extend u64: core.ops.assign.bit_xor_assign(u64) { let bit_xor_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self ^ rhs } }
extend u64: core.ops.assign.shl_assign(u64) { let shl_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self << rhs } }
extend u64: core.ops.assign.shr_assign(u64) { let shr_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self >> rhs } }
