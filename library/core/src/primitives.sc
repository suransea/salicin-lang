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
extend bool: core.ops.bit.not_operator {
  let output = bool
  let not(self)(): bool = {
    match self
      { false -> true }
      { true -> false }
  }
}
extend bool: core.cmp.equality(bool) {
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
extend i8: core.ops.arith.add_operator(i8) { let output = i8; let add(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.arith.sub_operator(i8) { let output = i8; let sub(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.arith.mul_operator(i8) { let output = i8; let mul(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.arith.div_operator(i8) { let output = i8; let div(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.arith.rem_operator(i8) { let output = i8; let rem(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.arith.neg_operator { let output = i8; let neg(self)(): i8 = { 0 - self } }
extend i8: core.ops.bit.bit_and_operator(i8) { let output = i8; let bit_and(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.bit.bit_or_operator(i8) { let output = i8; let bit_or(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.bit.bit_xor_operator(i8) { let output = i8; let bit_xor(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.bit.shl_operator(i8) { let output = i8; let shl(self)(rhs: i8): i8 = builtin() }
extend i8: core.ops.bit.shr_operator(i8) { let output = i8; let shr(self)(rhs: i8): i8 = builtin() }
extend i8: core.cmp.equality(i8) { let eq(self: borrow(i8))(rhs: borrow(i8)): bool = builtin() }
extend i8: core.cmp.partial_order(i8) { let partial_cmp(self: borrow(i8))(rhs: borrow(i8)): core.cmp.partial_ordering = builtin() }
extend i8: core.ops.assign.add_assign_operator(i8) { let add_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self + rhs } }
extend i8: core.ops.assign.sub_assign_operator(i8) { let sub_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self - rhs } }
extend i8: core.ops.assign.mul_assign_operator(i8) { let mul_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self * rhs } }
extend i8: core.ops.assign.div_assign_operator(i8) { let div_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self / rhs } }
extend i8: core.ops.assign.rem_assign_operator(i8) { let rem_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self % rhs } }
extend i8: core.ops.assign.bit_and_assign_operator(i8) { let bit_and_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self & rhs } }
extend i8: core.ops.assign.bit_or_assign_operator(i8) { let bit_or_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self | rhs } }
extend i8: core.ops.assign.bit_xor_assign_operator(i8) { let bit_xor_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self ^ rhs } }
extend i8: core.ops.assign.shl_assign_operator(i8) { let shl_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self << rhs } }
extend i8: core.ops.assign.shr_assign_operator(i8) { let shr_assign(self: borrow(mut)(i8))(rhs: i8): () = { self = self >> rhs } }

extend i16: core.marker.copyable {}
extend i16: core.ops.arith.add_operator(i16) { let output = i16; let add(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.arith.sub_operator(i16) { let output = i16; let sub(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.arith.mul_operator(i16) { let output = i16; let mul(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.arith.div_operator(i16) { let output = i16; let div(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.arith.rem_operator(i16) { let output = i16; let rem(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.arith.neg_operator { let output = i16; let neg(self)(): i16 = { 0 - self } }
extend i16: core.ops.bit.bit_and_operator(i16) { let output = i16; let bit_and(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.bit.bit_or_operator(i16) { let output = i16; let bit_or(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.bit.bit_xor_operator(i16) { let output = i16; let bit_xor(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.bit.shl_operator(i16) { let output = i16; let shl(self)(rhs: i16): i16 = builtin() }
extend i16: core.ops.bit.shr_operator(i16) { let output = i16; let shr(self)(rhs: i16): i16 = builtin() }
extend i16: core.cmp.equality(i16) { let eq(self: borrow(i16))(rhs: borrow(i16)): bool = builtin() }
extend i16: core.cmp.partial_order(i16) { let partial_cmp(self: borrow(i16))(rhs: borrow(i16)): core.cmp.partial_ordering = builtin() }
extend i16: core.ops.assign.add_assign_operator(i16) { let add_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self + rhs } }
extend i16: core.ops.assign.sub_assign_operator(i16) { let sub_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self - rhs } }
extend i16: core.ops.assign.mul_assign_operator(i16) { let mul_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self * rhs } }
extend i16: core.ops.assign.div_assign_operator(i16) { let div_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self / rhs } }
extend i16: core.ops.assign.rem_assign_operator(i16) { let rem_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self % rhs } }
extend i16: core.ops.assign.bit_and_assign_operator(i16) { let bit_and_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self & rhs } }
extend i16: core.ops.assign.bit_or_assign_operator(i16) { let bit_or_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self | rhs } }
extend i16: core.ops.assign.bit_xor_assign_operator(i16) { let bit_xor_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self ^ rhs } }
extend i16: core.ops.assign.shl_assign_operator(i16) { let shl_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self << rhs } }
extend i16: core.ops.assign.shr_assign_operator(i16) { let shr_assign(self: borrow(mut)(i16))(rhs: i16): () = { self = self >> rhs } }

extend i128: core.marker.copyable {}
extend i128: core.ops.arith.add_operator(i128) { let output = i128; let add(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.arith.sub_operator(i128) { let output = i128; let sub(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.arith.mul_operator(i128) { let output = i128; let mul(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.arith.div_operator(i128) { let output = i128; let div(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.arith.rem_operator(i128) { let output = i128; let rem(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.arith.neg_operator { let output = i128; let neg(self)(): i128 = { 0 - self } }
extend i128: core.ops.bit.bit_and_operator(i128) { let output = i128; let bit_and(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.bit.bit_or_operator(i128) { let output = i128; let bit_or(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.bit.bit_xor_operator(i128) { let output = i128; let bit_xor(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.bit.shl_operator(i128) { let output = i128; let shl(self)(rhs: i128): i128 = builtin() }
extend i128: core.ops.bit.shr_operator(i128) { let output = i128; let shr(self)(rhs: i128): i128 = builtin() }
extend i128: core.cmp.equality(i128) { let eq(self: borrow(i128))(rhs: borrow(i128)): bool = builtin() }
extend i128: core.cmp.partial_order(i128) { let partial_cmp(self: borrow(i128))(rhs: borrow(i128)): core.cmp.partial_ordering = builtin() }
extend i128: core.ops.assign.add_assign_operator(i128) { let add_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self + rhs } }
extend i128: core.ops.assign.sub_assign_operator(i128) { let sub_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self - rhs } }
extend i128: core.ops.assign.mul_assign_operator(i128) { let mul_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self * rhs } }
extend i128: core.ops.assign.div_assign_operator(i128) { let div_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self / rhs } }
extend i128: core.ops.assign.rem_assign_operator(i128) { let rem_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self % rhs } }
extend i128: core.ops.assign.bit_and_assign_operator(i128) { let bit_and_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self & rhs } }
extend i128: core.ops.assign.bit_or_assign_operator(i128) { let bit_or_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self | rhs } }
extend i128: core.ops.assign.bit_xor_assign_operator(i128) { let bit_xor_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self ^ rhs } }
extend i128: core.ops.assign.shl_assign_operator(i128) { let shl_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self << rhs } }
extend i128: core.ops.assign.shr_assign_operator(i128) { let shr_assign(self: borrow(mut)(i128))(rhs: i128): () = { self = self >> rhs } }

extend isize: core.marker.copyable {}
extend isize: core.ops.arith.add_operator(isize) { let output = isize; let add(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.arith.sub_operator(isize) { let output = isize; let sub(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.arith.mul_operator(isize) { let output = isize; let mul(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.arith.div_operator(isize) { let output = isize; let div(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.arith.rem_operator(isize) { let output = isize; let rem(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.arith.neg_operator { let output = isize; let neg(self)(): isize = { 0 - self } }
extend isize: core.ops.bit.bit_and_operator(isize) { let output = isize; let bit_and(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.bit.bit_or_operator(isize) { let output = isize; let bit_or(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.bit.bit_xor_operator(isize) { let output = isize; let bit_xor(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.bit.shl_operator(isize) { let output = isize; let shl(self)(rhs: isize): isize = builtin() }
extend isize: core.ops.bit.shr_operator(isize) { let output = isize; let shr(self)(rhs: isize): isize = builtin() }
extend isize: core.cmp.equality(isize) { let eq(self: borrow(isize))(rhs: borrow(isize)): bool = builtin() }
extend isize: core.cmp.partial_order(isize) { let partial_cmp(self: borrow(isize))(rhs: borrow(isize)): core.cmp.partial_ordering = builtin() }
extend isize: core.ops.assign.add_assign_operator(isize) { let add_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self + rhs } }
extend isize: core.ops.assign.sub_assign_operator(isize) { let sub_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self - rhs } }
extend isize: core.ops.assign.mul_assign_operator(isize) { let mul_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self * rhs } }
extend isize: core.ops.assign.div_assign_operator(isize) { let div_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self / rhs } }
extend isize: core.ops.assign.rem_assign_operator(isize) { let rem_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self % rhs } }
extend isize: core.ops.assign.bit_and_assign_operator(isize) { let bit_and_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self & rhs } }
extend isize: core.ops.assign.bit_or_assign_operator(isize) { let bit_or_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self | rhs } }
extend isize: core.ops.assign.bit_xor_assign_operator(isize) { let bit_xor_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self ^ rhs } }
extend isize: core.ops.assign.shl_assign_operator(isize) { let shl_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self << rhs } }
extend isize: core.ops.assign.shr_assign_operator(isize) { let shr_assign(self: borrow(mut)(isize))(rhs: isize): () = { self = self >> rhs } }

extend u8: core.marker.copyable {}
extend u8: core.ops.arith.add_operator(u8) { let output = u8; let add(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.arith.sub_operator(u8) { let output = u8; let sub(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.arith.mul_operator(u8) { let output = u8; let mul(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.arith.div_operator(u8) { let output = u8; let div(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.arith.rem_operator(u8) { let output = u8; let rem(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.bit.bit_and_operator(u8) { let output = u8; let bit_and(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.bit.bit_or_operator(u8) { let output = u8; let bit_or(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.bit.bit_xor_operator(u8) { let output = u8; let bit_xor(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.bit.shl_operator(u8) { let output = u8; let shl(self)(rhs: u8): u8 = builtin() }
extend u8: core.ops.bit.shr_operator(u8) { let output = u8; let shr(self)(rhs: u8): u8 = builtin() }
extend u8: core.cmp.equality(u8) { let eq(self: borrow(u8))(rhs: borrow(u8)): bool = builtin() }
extend u8: core.cmp.partial_order(u8) { let partial_cmp(self: borrow(u8))(rhs: borrow(u8)): core.cmp.partial_ordering = builtin() }
extend u8: core.ops.assign.add_assign_operator(u8) { let add_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self + rhs } }
extend u8: core.ops.assign.sub_assign_operator(u8) { let sub_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self - rhs } }
extend u8: core.ops.assign.mul_assign_operator(u8) { let mul_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self * rhs } }
extend u8: core.ops.assign.div_assign_operator(u8) { let div_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self / rhs } }
extend u8: core.ops.assign.rem_assign_operator(u8) { let rem_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self % rhs } }
extend u8: core.ops.assign.bit_and_assign_operator(u8) { let bit_and_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self & rhs } }
extend u8: core.ops.assign.bit_or_assign_operator(u8) { let bit_or_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self | rhs } }
extend u8: core.ops.assign.bit_xor_assign_operator(u8) { let bit_xor_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self ^ rhs } }
extend u8: core.ops.assign.shl_assign_operator(u8) { let shl_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self << rhs } }
extend u8: core.ops.assign.shr_assign_operator(u8) { let shr_assign(self: borrow(mut)(u8))(rhs: u8): () = { self = self >> rhs } }

extend u16: core.marker.copyable {}
extend u16: core.ops.arith.add_operator(u16) { let output = u16; let add(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.arith.sub_operator(u16) { let output = u16; let sub(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.arith.mul_operator(u16) { let output = u16; let mul(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.arith.div_operator(u16) { let output = u16; let div(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.arith.rem_operator(u16) { let output = u16; let rem(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.bit.bit_and_operator(u16) { let output = u16; let bit_and(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.bit.bit_or_operator(u16) { let output = u16; let bit_or(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.bit.bit_xor_operator(u16) { let output = u16; let bit_xor(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.bit.shl_operator(u16) { let output = u16; let shl(self)(rhs: u16): u16 = builtin() }
extend u16: core.ops.bit.shr_operator(u16) { let output = u16; let shr(self)(rhs: u16): u16 = builtin() }
extend u16: core.cmp.equality(u16) { let eq(self: borrow(u16))(rhs: borrow(u16)): bool = builtin() }
extend u16: core.cmp.partial_order(u16) { let partial_cmp(self: borrow(u16))(rhs: borrow(u16)): core.cmp.partial_ordering = builtin() }
extend u16: core.ops.assign.add_assign_operator(u16) { let add_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self + rhs } }
extend u16: core.ops.assign.sub_assign_operator(u16) { let sub_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self - rhs } }
extend u16: core.ops.assign.mul_assign_operator(u16) { let mul_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self * rhs } }
extend u16: core.ops.assign.div_assign_operator(u16) { let div_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self / rhs } }
extend u16: core.ops.assign.rem_assign_operator(u16) { let rem_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self % rhs } }
extend u16: core.ops.assign.bit_and_assign_operator(u16) { let bit_and_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self & rhs } }
extend u16: core.ops.assign.bit_or_assign_operator(u16) { let bit_or_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self | rhs } }
extend u16: core.ops.assign.bit_xor_assign_operator(u16) { let bit_xor_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self ^ rhs } }
extend u16: core.ops.assign.shl_assign_operator(u16) { let shl_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self << rhs } }
extend u16: core.ops.assign.shr_assign_operator(u16) { let shr_assign(self: borrow(mut)(u16))(rhs: u16): () = { self = self >> rhs } }

extend u128: core.marker.copyable {}
extend u128: core.ops.arith.add_operator(u128) { let output = u128; let add(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.arith.sub_operator(u128) { let output = u128; let sub(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.arith.mul_operator(u128) { let output = u128; let mul(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.arith.div_operator(u128) { let output = u128; let div(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.arith.rem_operator(u128) { let output = u128; let rem(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.bit.bit_and_operator(u128) { let output = u128; let bit_and(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.bit.bit_or_operator(u128) { let output = u128; let bit_or(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.bit.bit_xor_operator(u128) { let output = u128; let bit_xor(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.bit.shl_operator(u128) { let output = u128; let shl(self)(rhs: u128): u128 = builtin() }
extend u128: core.ops.bit.shr_operator(u128) { let output = u128; let shr(self)(rhs: u128): u128 = builtin() }
extend u128: core.cmp.equality(u128) { let eq(self: borrow(u128))(rhs: borrow(u128)): bool = builtin() }
extend u128: core.cmp.partial_order(u128) { let partial_cmp(self: borrow(u128))(rhs: borrow(u128)): core.cmp.partial_ordering = builtin() }
extend u128: core.ops.assign.add_assign_operator(u128) { let add_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self + rhs } }
extend u128: core.ops.assign.sub_assign_operator(u128) { let sub_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self - rhs } }
extend u128: core.ops.assign.mul_assign_operator(u128) { let mul_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self * rhs } }
extend u128: core.ops.assign.div_assign_operator(u128) { let div_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self / rhs } }
extend u128: core.ops.assign.rem_assign_operator(u128) { let rem_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self % rhs } }
extend u128: core.ops.assign.bit_and_assign_operator(u128) { let bit_and_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self & rhs } }
extend u128: core.ops.assign.bit_or_assign_operator(u128) { let bit_or_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self | rhs } }
extend u128: core.ops.assign.bit_xor_assign_operator(u128) { let bit_xor_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self ^ rhs } }
extend u128: core.ops.assign.shl_assign_operator(u128) { let shl_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self << rhs } }
extend u128: core.ops.assign.shr_assign_operator(u128) { let shr_assign(self: borrow(mut)(u128))(rhs: u128): () = { self = self >> rhs } }

extend usize: core.marker.copyable {}
extend usize: core.ops.arith.add_operator(usize) { let output = usize; let add(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.arith.sub_operator(usize) { let output = usize; let sub(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.arith.mul_operator(usize) { let output = usize; let mul(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.arith.div_operator(usize) { let output = usize; let div(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.arith.rem_operator(usize) { let output = usize; let rem(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.bit.bit_and_operator(usize) { let output = usize; let bit_and(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.bit.bit_or_operator(usize) { let output = usize; let bit_or(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.bit.bit_xor_operator(usize) { let output = usize; let bit_xor(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.bit.shl_operator(usize) { let output = usize; let shl(self)(rhs: usize): usize = builtin() }
extend usize: core.ops.bit.shr_operator(usize) { let output = usize; let shr(self)(rhs: usize): usize = builtin() }
extend usize: core.cmp.equality(usize) { let eq(self: borrow(usize))(rhs: borrow(usize)): bool = builtin() }
extend usize: core.cmp.partial_order(usize) { let partial_cmp(self: borrow(usize))(rhs: borrow(usize)): core.cmp.partial_ordering = builtin() }
extend usize: core.ops.assign.add_assign_operator(usize) { let add_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self + rhs } }
extend usize: core.ops.assign.sub_assign_operator(usize) { let sub_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self - rhs } }
extend usize: core.ops.assign.mul_assign_operator(usize) { let mul_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self * rhs } }
extend usize: core.ops.assign.div_assign_operator(usize) { let div_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self / rhs } }
extend usize: core.ops.assign.rem_assign_operator(usize) { let rem_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self % rhs } }
extend usize: core.ops.assign.bit_and_assign_operator(usize) { let bit_and_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self & rhs } }
extend usize: core.ops.assign.bit_or_assign_operator(usize) { let bit_or_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self | rhs } }
extend usize: core.ops.assign.bit_xor_assign_operator(usize) { let bit_xor_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self ^ rhs } }
extend usize: core.ops.assign.shl_assign_operator(usize) { let shl_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self << rhs } }
extend usize: core.ops.assign.shr_assign_operator(usize) { let shr_assign(self: borrow(mut)(usize))(rhs: usize): () = { self = self >> rhs } }

extend i32: core.marker.copyable {}
extend i32: core.ops.arith.add_operator(i32) { let output = i32; let add(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.arith.sub_operator(i32) { let output = i32; let sub(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.arith.mul_operator(i32) { let output = i32; let mul(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.arith.div_operator(i32) { let output = i32; let div(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.arith.rem_operator(i32) { let output = i32; let rem(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.arith.neg_operator { let output = i32; let neg(self)(): i32 = { 0 - self } }
extend i32: core.ops.bit.bit_and_operator(i32) { let output = i32; let bit_and(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.bit.bit_or_operator(i32) { let output = i32; let bit_or(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.bit.bit_xor_operator(i32) { let output = i32; let bit_xor(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.bit.shl_operator(i32) { let output = i32; let shl(self)(rhs: i32): i32 = builtin() }
extend i32: core.ops.bit.shr_operator(i32) { let output = i32; let shr(self)(rhs: i32): i32 = builtin() }
extend i32: core.cmp.equality(i32) { let eq(self: borrow(i32))(rhs: borrow(i32)): bool = builtin() }
extend i32: core.cmp.partial_order(i32) { let partial_cmp(self: borrow(i32))(rhs: borrow(i32)): core.cmp.partial_ordering = builtin() }
extend i32: core.ops.assign.add_assign_operator(i32) { let add_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self + rhs } }
extend i32: core.ops.assign.sub_assign_operator(i32) { let sub_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self - rhs } }
extend i32: core.ops.assign.mul_assign_operator(i32) { let mul_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self * rhs } }
extend i32: core.ops.assign.div_assign_operator(i32) { let div_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self / rhs } }
extend i32: core.ops.assign.rem_assign_operator(i32) { let rem_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self % rhs } }
extend i32: core.ops.assign.bit_and_assign_operator(i32) { let bit_and_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self & rhs } }
extend i32: core.ops.assign.bit_or_assign_operator(i32) { let bit_or_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self | rhs } }
extend i32: core.ops.assign.bit_xor_assign_operator(i32) { let bit_xor_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self ^ rhs } }
extend i32: core.ops.assign.shl_assign_operator(i32) { let shl_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self << rhs } }
extend i32: core.ops.assign.shr_assign_operator(i32) { let shr_assign(self: borrow(mut)(i32))(rhs: i32): () = { self = self >> rhs } }

extend i64: core.marker.copyable {}
extend i64: core.ops.arith.add_operator(i64) { let output = i64; let add(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.arith.sub_operator(i64) { let output = i64; let sub(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.arith.mul_operator(i64) { let output = i64; let mul(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.arith.div_operator(i64) { let output = i64; let div(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.arith.rem_operator(i64) { let output = i64; let rem(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.arith.neg_operator { let output = i64; let neg(self)(): i64 = { 0 - self } }
extend i64: core.ops.bit.bit_and_operator(i64) { let output = i64; let bit_and(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.bit.bit_or_operator(i64) { let output = i64; let bit_or(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.bit.bit_xor_operator(i64) { let output = i64; let bit_xor(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.bit.shl_operator(i64) { let output = i64; let shl(self)(rhs: i64): i64 = builtin() }
extend i64: core.ops.bit.shr_operator(i64) { let output = i64; let shr(self)(rhs: i64): i64 = builtin() }
extend i64: core.cmp.equality(i64) { let eq(self: borrow(i64))(rhs: borrow(i64)): bool = builtin() }
extend i64: core.cmp.partial_order(i64) { let partial_cmp(self: borrow(i64))(rhs: borrow(i64)): core.cmp.partial_ordering = builtin() }
extend i64: core.ops.assign.add_assign_operator(i64) { let add_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self + rhs } }
extend i64: core.ops.assign.sub_assign_operator(i64) { let sub_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self - rhs } }
extend i64: core.ops.assign.mul_assign_operator(i64) { let mul_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self * rhs } }
extend i64: core.ops.assign.div_assign_operator(i64) { let div_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self / rhs } }
extend i64: core.ops.assign.rem_assign_operator(i64) { let rem_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self % rhs } }
extend i64: core.ops.assign.bit_and_assign_operator(i64) { let bit_and_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self & rhs } }
extend i64: core.ops.assign.bit_or_assign_operator(i64) { let bit_or_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self | rhs } }
extend i64: core.ops.assign.bit_xor_assign_operator(i64) { let bit_xor_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self ^ rhs } }
extend i64: core.ops.assign.shl_assign_operator(i64) { let shl_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self << rhs } }
extend i64: core.ops.assign.shr_assign_operator(i64) { let shr_assign(self: borrow(mut)(i64))(rhs: i64): () = { self = self >> rhs } }

extend u32: core.marker.copyable {}
extend u32: core.ops.arith.add_operator(u32) { let output = u32; let add(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.arith.sub_operator(u32) { let output = u32; let sub(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.arith.mul_operator(u32) { let output = u32; let mul(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.arith.div_operator(u32) { let output = u32; let div(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.arith.rem_operator(u32) { let output = u32; let rem(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.bit.bit_and_operator(u32) { let output = u32; let bit_and(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.bit.bit_or_operator(u32) { let output = u32; let bit_or(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.bit.bit_xor_operator(u32) { let output = u32; let bit_xor(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.bit.shl_operator(u32) { let output = u32; let shl(self)(rhs: u32): u32 = builtin() }
extend u32: core.ops.bit.shr_operator(u32) { let output = u32; let shr(self)(rhs: u32): u32 = builtin() }
extend u32: core.cmp.equality(u32) { let eq(self: borrow(u32))(rhs: borrow(u32)): bool = builtin() }
extend u32: core.cmp.partial_order(u32) { let partial_cmp(self: borrow(u32))(rhs: borrow(u32)): core.cmp.partial_ordering = builtin() }
extend u32: core.ops.assign.add_assign_operator(u32) { let add_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self + rhs } }
extend u32: core.ops.assign.sub_assign_operator(u32) { let sub_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self - rhs } }
extend u32: core.ops.assign.mul_assign_operator(u32) { let mul_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self * rhs } }
extend u32: core.ops.assign.div_assign_operator(u32) { let div_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self / rhs } }
extend u32: core.ops.assign.rem_assign_operator(u32) { let rem_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self % rhs } }
extend u32: core.ops.assign.bit_and_assign_operator(u32) { let bit_and_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self & rhs } }
extend u32: core.ops.assign.bit_or_assign_operator(u32) { let bit_or_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self | rhs } }
extend u32: core.ops.assign.bit_xor_assign_operator(u32) { let bit_xor_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self ^ rhs } }
extend u32: core.ops.assign.shl_assign_operator(u32) { let shl_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self << rhs } }
extend u32: core.ops.assign.shr_assign_operator(u32) { let shr_assign(self: borrow(mut)(u32))(rhs: u32): () = { self = self >> rhs } }

extend u64: core.marker.copyable {}
extend u64: core.ops.arith.add_operator(u64) { let output = u64; let add(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.arith.sub_operator(u64) { let output = u64; let sub(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.arith.mul_operator(u64) { let output = u64; let mul(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.arith.div_operator(u64) { let output = u64; let div(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.arith.rem_operator(u64) { let output = u64; let rem(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.bit.bit_and_operator(u64) { let output = u64; let bit_and(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.bit.bit_or_operator(u64) { let output = u64; let bit_or(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.bit.bit_xor_operator(u64) { let output = u64; let bit_xor(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.bit.shl_operator(u64) { let output = u64; let shl(self)(rhs: u64): u64 = builtin() }
extend u64: core.ops.bit.shr_operator(u64) { let output = u64; let shr(self)(rhs: u64): u64 = builtin() }
extend u64: core.cmp.equality(u64) { let eq(self: borrow(u64))(rhs: borrow(u64)): bool = builtin() }
extend u64: core.cmp.partial_order(u64) { let partial_cmp(self: borrow(u64))(rhs: borrow(u64)): core.cmp.partial_ordering = builtin() }
extend u64: core.ops.assign.add_assign_operator(u64) { let add_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self + rhs } }
extend u64: core.ops.assign.sub_assign_operator(u64) { let sub_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self - rhs } }
extend u64: core.ops.assign.mul_assign_operator(u64) { let mul_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self * rhs } }
extend u64: core.ops.assign.div_assign_operator(u64) { let div_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self / rhs } }
extend u64: core.ops.assign.rem_assign_operator(u64) { let rem_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self % rhs } }
extend u64: core.ops.assign.bit_and_assign_operator(u64) { let bit_and_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self & rhs } }
extend u64: core.ops.assign.bit_or_assign_operator(u64) { let bit_or_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self | rhs } }
extend u64: core.ops.assign.bit_xor_assign_operator(u64) { let bit_xor_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self ^ rhs } }
extend u64: core.ops.assign.shl_assign_operator(u64) { let shl_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self << rhs } }
extend u64: core.ops.assign.shr_assign_operator(u64) { let shr_assign(self: borrow(mut)(u64))(rhs: u64): () = { self = self >> rhs } }
