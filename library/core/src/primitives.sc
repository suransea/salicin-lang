// Opaque primitive types. Their source identities and contracts participate in
// ordinary type and trait resolution; the compiler only supplies representation
// and intrinsic lowering after validating this core bundle.
pub let bool = enum { false, true }
pub let i8: type
pub let i16: type
pub let i32: type
pub let i64: type
pub let i128: type
pub let isize: type
pub let u8: type
pub let u16: type
pub let u32: type
pub let u64: type
pub let u128: type
pub let usize: type

extend bool: core.marker.Copy {}
extend bool: core.ops.bit.Not {
  let Output = bool
  let not(self)(): bool
}
extend bool: core.cmp.Eq(bool) {
  let eq(self: borrow(bool))(rhs: borrow(bool)): bool
}

// Integer contracts are deliberately stated in source. A bodyless method in
// this core-only section names a compiler intrinsic; it is not an implicit impl.
extend i8: core.marker.Copy {}
extend i8: core.ops.arith.Add(i8) { let Output = i8; let add(self)(rhs: i8): i8 }
extend i8: core.ops.arith.Sub(i8) { let Output = i8; let sub(self)(rhs: i8): i8 }
extend i8: core.ops.arith.Mul(i8) { let Output = i8; let mul(self)(rhs: i8): i8 }
extend i8: core.ops.arith.Div(i8) { let Output = i8; let div(self)(rhs: i8): i8 }
extend i8: core.ops.arith.Rem(i8) { let Output = i8; let rem(self)(rhs: i8): i8 }
extend i8: core.ops.arith.Neg { let Output = i8; let neg(self)(): i8 }
extend i8: core.ops.bit.BitAnd(i8) { let Output = i8; let bit_and(self)(rhs: i8): i8 }
extend i8: core.ops.bit.BitOr(i8) { let Output = i8; let bit_or(self)(rhs: i8): i8 }
extend i8: core.ops.bit.BitXor(i8) { let Output = i8; let bit_xor(self)(rhs: i8): i8 }
extend i8: core.ops.bit.Shl(i8) { let Output = i8; let shl(self)(rhs: i8): i8 }
extend i8: core.ops.bit.Shr(i8) { let Output = i8; let shr(self)(rhs: i8): i8 }
extend i8: core.cmp.Eq(i8) { let eq(self: borrow(i8))(rhs: borrow(i8)): bool }
extend i8: core.cmp.PartialOrd(i8) { let partial_cmp(self: borrow(i8))(rhs: borrow(i8)): core.cmp.PartialOrdering }
extend i8: core.ops.assign.AddAssign(i8) { let add_assign(self: borrow(mut)(i8))(rhs: i8): () }
extend i8: core.ops.assign.SubAssign(i8) { let sub_assign(self: borrow(mut)(i8))(rhs: i8): () }
extend i8: core.ops.assign.MulAssign(i8) { let mul_assign(self: borrow(mut)(i8))(rhs: i8): () }
extend i8: core.ops.assign.DivAssign(i8) { let div_assign(self: borrow(mut)(i8))(rhs: i8): () }
extend i8: core.ops.assign.RemAssign(i8) { let rem_assign(self: borrow(mut)(i8))(rhs: i8): () }
extend i8: core.ops.assign.BitAndAssign(i8) { let bit_and_assign(self: borrow(mut)(i8))(rhs: i8): () }
extend i8: core.ops.assign.BitOrAssign(i8) { let bit_or_assign(self: borrow(mut)(i8))(rhs: i8): () }
extend i8: core.ops.assign.BitXorAssign(i8) { let bit_xor_assign(self: borrow(mut)(i8))(rhs: i8): () }
extend i8: core.ops.assign.ShlAssign(i8) { let shl_assign(self: borrow(mut)(i8))(rhs: i8): () }
extend i8: core.ops.assign.ShrAssign(i8) { let shr_assign(self: borrow(mut)(i8))(rhs: i8): () }

extend i16: core.marker.Copy {}
extend i16: core.ops.arith.Add(i16) { let Output = i16; let add(self)(rhs: i16): i16 }
extend i16: core.ops.arith.Sub(i16) { let Output = i16; let sub(self)(rhs: i16): i16 }
extend i16: core.ops.arith.Mul(i16) { let Output = i16; let mul(self)(rhs: i16): i16 }
extend i16: core.ops.arith.Div(i16) { let Output = i16; let div(self)(rhs: i16): i16 }
extend i16: core.ops.arith.Rem(i16) { let Output = i16; let rem(self)(rhs: i16): i16 }
extend i16: core.ops.arith.Neg { let Output = i16; let neg(self)(): i16 }
extend i16: core.ops.bit.BitAnd(i16) { let Output = i16; let bit_and(self)(rhs: i16): i16 }
extend i16: core.ops.bit.BitOr(i16) { let Output = i16; let bit_or(self)(rhs: i16): i16 }
extend i16: core.ops.bit.BitXor(i16) { let Output = i16; let bit_xor(self)(rhs: i16): i16 }
extend i16: core.ops.bit.Shl(i16) { let Output = i16; let shl(self)(rhs: i16): i16 }
extend i16: core.ops.bit.Shr(i16) { let Output = i16; let shr(self)(rhs: i16): i16 }
extend i16: core.cmp.Eq(i16) { let eq(self: borrow(i16))(rhs: borrow(i16)): bool }
extend i16: core.cmp.PartialOrd(i16) { let partial_cmp(self: borrow(i16))(rhs: borrow(i16)): core.cmp.PartialOrdering }
extend i16: core.ops.assign.AddAssign(i16) { let add_assign(self: borrow(mut)(i16))(rhs: i16): () }
extend i16: core.ops.assign.SubAssign(i16) { let sub_assign(self: borrow(mut)(i16))(rhs: i16): () }
extend i16: core.ops.assign.MulAssign(i16) { let mul_assign(self: borrow(mut)(i16))(rhs: i16): () }
extend i16: core.ops.assign.DivAssign(i16) { let div_assign(self: borrow(mut)(i16))(rhs: i16): () }
extend i16: core.ops.assign.RemAssign(i16) { let rem_assign(self: borrow(mut)(i16))(rhs: i16): () }
extend i16: core.ops.assign.BitAndAssign(i16) { let bit_and_assign(self: borrow(mut)(i16))(rhs: i16): () }
extend i16: core.ops.assign.BitOrAssign(i16) { let bit_or_assign(self: borrow(mut)(i16))(rhs: i16): () }
extend i16: core.ops.assign.BitXorAssign(i16) { let bit_xor_assign(self: borrow(mut)(i16))(rhs: i16): () }
extend i16: core.ops.assign.ShlAssign(i16) { let shl_assign(self: borrow(mut)(i16))(rhs: i16): () }
extend i16: core.ops.assign.ShrAssign(i16) { let shr_assign(self: borrow(mut)(i16))(rhs: i16): () }

extend i128: core.marker.Copy {}
extend i128: core.ops.arith.Add(i128) { let Output = i128; let add(self)(rhs: i128): i128 }
extend i128: core.ops.arith.Sub(i128) { let Output = i128; let sub(self)(rhs: i128): i128 }
extend i128: core.ops.arith.Mul(i128) { let Output = i128; let mul(self)(rhs: i128): i128 }
extend i128: core.ops.arith.Div(i128) { let Output = i128; let div(self)(rhs: i128): i128 }
extend i128: core.ops.arith.Rem(i128) { let Output = i128; let rem(self)(rhs: i128): i128 }
extend i128: core.ops.arith.Neg { let Output = i128; let neg(self)(): i128 }
extend i128: core.ops.bit.BitAnd(i128) { let Output = i128; let bit_and(self)(rhs: i128): i128 }
extend i128: core.ops.bit.BitOr(i128) { let Output = i128; let bit_or(self)(rhs: i128): i128 }
extend i128: core.ops.bit.BitXor(i128) { let Output = i128; let bit_xor(self)(rhs: i128): i128 }
extend i128: core.ops.bit.Shl(i128) { let Output = i128; let shl(self)(rhs: i128): i128 }
extend i128: core.ops.bit.Shr(i128) { let Output = i128; let shr(self)(rhs: i128): i128 }
extend i128: core.cmp.Eq(i128) { let eq(self: borrow(i128))(rhs: borrow(i128)): bool }
extend i128: core.cmp.PartialOrd(i128) { let partial_cmp(self: borrow(i128))(rhs: borrow(i128)): core.cmp.PartialOrdering }
extend i128: core.ops.assign.AddAssign(i128) { let add_assign(self: borrow(mut)(i128))(rhs: i128): () }
extend i128: core.ops.assign.SubAssign(i128) { let sub_assign(self: borrow(mut)(i128))(rhs: i128): () }
extend i128: core.ops.assign.MulAssign(i128) { let mul_assign(self: borrow(mut)(i128))(rhs: i128): () }
extend i128: core.ops.assign.DivAssign(i128) { let div_assign(self: borrow(mut)(i128))(rhs: i128): () }
extend i128: core.ops.assign.RemAssign(i128) { let rem_assign(self: borrow(mut)(i128))(rhs: i128): () }
extend i128: core.ops.assign.BitAndAssign(i128) { let bit_and_assign(self: borrow(mut)(i128))(rhs: i128): () }
extend i128: core.ops.assign.BitOrAssign(i128) { let bit_or_assign(self: borrow(mut)(i128))(rhs: i128): () }
extend i128: core.ops.assign.BitXorAssign(i128) { let bit_xor_assign(self: borrow(mut)(i128))(rhs: i128): () }
extend i128: core.ops.assign.ShlAssign(i128) { let shl_assign(self: borrow(mut)(i128))(rhs: i128): () }
extend i128: core.ops.assign.ShrAssign(i128) { let shr_assign(self: borrow(mut)(i128))(rhs: i128): () }

extend isize: core.marker.Copy {}
extend isize: core.ops.arith.Add(isize) { let Output = isize; let add(self)(rhs: isize): isize }
extend isize: core.ops.arith.Sub(isize) { let Output = isize; let sub(self)(rhs: isize): isize }
extend isize: core.ops.arith.Mul(isize) { let Output = isize; let mul(self)(rhs: isize): isize }
extend isize: core.ops.arith.Div(isize) { let Output = isize; let div(self)(rhs: isize): isize }
extend isize: core.ops.arith.Rem(isize) { let Output = isize; let rem(self)(rhs: isize): isize }
extend isize: core.ops.arith.Neg { let Output = isize; let neg(self)(): isize }
extend isize: core.ops.bit.BitAnd(isize) { let Output = isize; let bit_and(self)(rhs: isize): isize }
extend isize: core.ops.bit.BitOr(isize) { let Output = isize; let bit_or(self)(rhs: isize): isize }
extend isize: core.ops.bit.BitXor(isize) { let Output = isize; let bit_xor(self)(rhs: isize): isize }
extend isize: core.ops.bit.Shl(isize) { let Output = isize; let shl(self)(rhs: isize): isize }
extend isize: core.ops.bit.Shr(isize) { let Output = isize; let shr(self)(rhs: isize): isize }
extend isize: core.cmp.Eq(isize) { let eq(self: borrow(isize))(rhs: borrow(isize)): bool }
extend isize: core.cmp.PartialOrd(isize) { let partial_cmp(self: borrow(isize))(rhs: borrow(isize)): core.cmp.PartialOrdering }
extend isize: core.ops.assign.AddAssign(isize) { let add_assign(self: borrow(mut)(isize))(rhs: isize): () }
extend isize: core.ops.assign.SubAssign(isize) { let sub_assign(self: borrow(mut)(isize))(rhs: isize): () }
extend isize: core.ops.assign.MulAssign(isize) { let mul_assign(self: borrow(mut)(isize))(rhs: isize): () }
extend isize: core.ops.assign.DivAssign(isize) { let div_assign(self: borrow(mut)(isize))(rhs: isize): () }
extend isize: core.ops.assign.RemAssign(isize) { let rem_assign(self: borrow(mut)(isize))(rhs: isize): () }
extend isize: core.ops.assign.BitAndAssign(isize) { let bit_and_assign(self: borrow(mut)(isize))(rhs: isize): () }
extend isize: core.ops.assign.BitOrAssign(isize) { let bit_or_assign(self: borrow(mut)(isize))(rhs: isize): () }
extend isize: core.ops.assign.BitXorAssign(isize) { let bit_xor_assign(self: borrow(mut)(isize))(rhs: isize): () }
extend isize: core.ops.assign.ShlAssign(isize) { let shl_assign(self: borrow(mut)(isize))(rhs: isize): () }
extend isize: core.ops.assign.ShrAssign(isize) { let shr_assign(self: borrow(mut)(isize))(rhs: isize): () }

extend u8: core.marker.Copy {}
extend u8: core.ops.arith.Add(u8) { let Output = u8; let add(self)(rhs: u8): u8 }
extend u8: core.ops.arith.Sub(u8) { let Output = u8; let sub(self)(rhs: u8): u8 }
extend u8: core.ops.arith.Mul(u8) { let Output = u8; let mul(self)(rhs: u8): u8 }
extend u8: core.ops.arith.Div(u8) { let Output = u8; let div(self)(rhs: u8): u8 }
extend u8: core.ops.arith.Rem(u8) { let Output = u8; let rem(self)(rhs: u8): u8 }
extend u8: core.ops.bit.BitAnd(u8) { let Output = u8; let bit_and(self)(rhs: u8): u8 }
extend u8: core.ops.bit.BitOr(u8) { let Output = u8; let bit_or(self)(rhs: u8): u8 }
extend u8: core.ops.bit.BitXor(u8) { let Output = u8; let bit_xor(self)(rhs: u8): u8 }
extend u8: core.ops.bit.Shl(u8) { let Output = u8; let shl(self)(rhs: u8): u8 }
extend u8: core.ops.bit.Shr(u8) { let Output = u8; let shr(self)(rhs: u8): u8 }
extend u8: core.cmp.Eq(u8) { let eq(self: borrow(u8))(rhs: borrow(u8)): bool }
extend u8: core.cmp.PartialOrd(u8) { let partial_cmp(self: borrow(u8))(rhs: borrow(u8)): core.cmp.PartialOrdering }
extend u8: core.ops.assign.AddAssign(u8) { let add_assign(self: borrow(mut)(u8))(rhs: u8): () }
extend u8: core.ops.assign.SubAssign(u8) { let sub_assign(self: borrow(mut)(u8))(rhs: u8): () }
extend u8: core.ops.assign.MulAssign(u8) { let mul_assign(self: borrow(mut)(u8))(rhs: u8): () }
extend u8: core.ops.assign.DivAssign(u8) { let div_assign(self: borrow(mut)(u8))(rhs: u8): () }
extend u8: core.ops.assign.RemAssign(u8) { let rem_assign(self: borrow(mut)(u8))(rhs: u8): () }
extend u8: core.ops.assign.BitAndAssign(u8) { let bit_and_assign(self: borrow(mut)(u8))(rhs: u8): () }
extend u8: core.ops.assign.BitOrAssign(u8) { let bit_or_assign(self: borrow(mut)(u8))(rhs: u8): () }
extend u8: core.ops.assign.BitXorAssign(u8) { let bit_xor_assign(self: borrow(mut)(u8))(rhs: u8): () }
extend u8: core.ops.assign.ShlAssign(u8) { let shl_assign(self: borrow(mut)(u8))(rhs: u8): () }
extend u8: core.ops.assign.ShrAssign(u8) { let shr_assign(self: borrow(mut)(u8))(rhs: u8): () }

extend u16: core.marker.Copy {}
extend u16: core.ops.arith.Add(u16) { let Output = u16; let add(self)(rhs: u16): u16 }
extend u16: core.ops.arith.Sub(u16) { let Output = u16; let sub(self)(rhs: u16): u16 }
extend u16: core.ops.arith.Mul(u16) { let Output = u16; let mul(self)(rhs: u16): u16 }
extend u16: core.ops.arith.Div(u16) { let Output = u16; let div(self)(rhs: u16): u16 }
extend u16: core.ops.arith.Rem(u16) { let Output = u16; let rem(self)(rhs: u16): u16 }
extend u16: core.ops.bit.BitAnd(u16) { let Output = u16; let bit_and(self)(rhs: u16): u16 }
extend u16: core.ops.bit.BitOr(u16) { let Output = u16; let bit_or(self)(rhs: u16): u16 }
extend u16: core.ops.bit.BitXor(u16) { let Output = u16; let bit_xor(self)(rhs: u16): u16 }
extend u16: core.ops.bit.Shl(u16) { let Output = u16; let shl(self)(rhs: u16): u16 }
extend u16: core.ops.bit.Shr(u16) { let Output = u16; let shr(self)(rhs: u16): u16 }
extend u16: core.cmp.Eq(u16) { let eq(self: borrow(u16))(rhs: borrow(u16)): bool }
extend u16: core.cmp.PartialOrd(u16) { let partial_cmp(self: borrow(u16))(rhs: borrow(u16)): core.cmp.PartialOrdering }
extend u16: core.ops.assign.AddAssign(u16) { let add_assign(self: borrow(mut)(u16))(rhs: u16): () }
extend u16: core.ops.assign.SubAssign(u16) { let sub_assign(self: borrow(mut)(u16))(rhs: u16): () }
extend u16: core.ops.assign.MulAssign(u16) { let mul_assign(self: borrow(mut)(u16))(rhs: u16): () }
extend u16: core.ops.assign.DivAssign(u16) { let div_assign(self: borrow(mut)(u16))(rhs: u16): () }
extend u16: core.ops.assign.RemAssign(u16) { let rem_assign(self: borrow(mut)(u16))(rhs: u16): () }
extend u16: core.ops.assign.BitAndAssign(u16) { let bit_and_assign(self: borrow(mut)(u16))(rhs: u16): () }
extend u16: core.ops.assign.BitOrAssign(u16) { let bit_or_assign(self: borrow(mut)(u16))(rhs: u16): () }
extend u16: core.ops.assign.BitXorAssign(u16) { let bit_xor_assign(self: borrow(mut)(u16))(rhs: u16): () }
extend u16: core.ops.assign.ShlAssign(u16) { let shl_assign(self: borrow(mut)(u16))(rhs: u16): () }
extend u16: core.ops.assign.ShrAssign(u16) { let shr_assign(self: borrow(mut)(u16))(rhs: u16): () }

extend u128: core.marker.Copy {}
extend u128: core.ops.arith.Add(u128) { let Output = u128; let add(self)(rhs: u128): u128 }
extend u128: core.ops.arith.Sub(u128) { let Output = u128; let sub(self)(rhs: u128): u128 }
extend u128: core.ops.arith.Mul(u128) { let Output = u128; let mul(self)(rhs: u128): u128 }
extend u128: core.ops.arith.Div(u128) { let Output = u128; let div(self)(rhs: u128): u128 }
extend u128: core.ops.arith.Rem(u128) { let Output = u128; let rem(self)(rhs: u128): u128 }
extend u128: core.ops.bit.BitAnd(u128) { let Output = u128; let bit_and(self)(rhs: u128): u128 }
extend u128: core.ops.bit.BitOr(u128) { let Output = u128; let bit_or(self)(rhs: u128): u128 }
extend u128: core.ops.bit.BitXor(u128) { let Output = u128; let bit_xor(self)(rhs: u128): u128 }
extend u128: core.ops.bit.Shl(u128) { let Output = u128; let shl(self)(rhs: u128): u128 }
extend u128: core.ops.bit.Shr(u128) { let Output = u128; let shr(self)(rhs: u128): u128 }
extend u128: core.cmp.Eq(u128) { let eq(self: borrow(u128))(rhs: borrow(u128)): bool }
extend u128: core.cmp.PartialOrd(u128) { let partial_cmp(self: borrow(u128))(rhs: borrow(u128)): core.cmp.PartialOrdering }
extend u128: core.ops.assign.AddAssign(u128) { let add_assign(self: borrow(mut)(u128))(rhs: u128): () }
extend u128: core.ops.assign.SubAssign(u128) { let sub_assign(self: borrow(mut)(u128))(rhs: u128): () }
extend u128: core.ops.assign.MulAssign(u128) { let mul_assign(self: borrow(mut)(u128))(rhs: u128): () }
extend u128: core.ops.assign.DivAssign(u128) { let div_assign(self: borrow(mut)(u128))(rhs: u128): () }
extend u128: core.ops.assign.RemAssign(u128) { let rem_assign(self: borrow(mut)(u128))(rhs: u128): () }
extend u128: core.ops.assign.BitAndAssign(u128) { let bit_and_assign(self: borrow(mut)(u128))(rhs: u128): () }
extend u128: core.ops.assign.BitOrAssign(u128) { let bit_or_assign(self: borrow(mut)(u128))(rhs: u128): () }
extend u128: core.ops.assign.BitXorAssign(u128) { let bit_xor_assign(self: borrow(mut)(u128))(rhs: u128): () }
extend u128: core.ops.assign.ShlAssign(u128) { let shl_assign(self: borrow(mut)(u128))(rhs: u128): () }
extend u128: core.ops.assign.ShrAssign(u128) { let shr_assign(self: borrow(mut)(u128))(rhs: u128): () }

extend usize: core.marker.Copy {}
extend usize: core.ops.arith.Add(usize) { let Output = usize; let add(self)(rhs: usize): usize }
extend usize: core.ops.arith.Sub(usize) { let Output = usize; let sub(self)(rhs: usize): usize }
extend usize: core.ops.arith.Mul(usize) { let Output = usize; let mul(self)(rhs: usize): usize }
extend usize: core.ops.arith.Div(usize) { let Output = usize; let div(self)(rhs: usize): usize }
extend usize: core.ops.arith.Rem(usize) { let Output = usize; let rem(self)(rhs: usize): usize }
extend usize: core.ops.bit.BitAnd(usize) { let Output = usize; let bit_and(self)(rhs: usize): usize }
extend usize: core.ops.bit.BitOr(usize) { let Output = usize; let bit_or(self)(rhs: usize): usize }
extend usize: core.ops.bit.BitXor(usize) { let Output = usize; let bit_xor(self)(rhs: usize): usize }
extend usize: core.ops.bit.Shl(usize) { let Output = usize; let shl(self)(rhs: usize): usize }
extend usize: core.ops.bit.Shr(usize) { let Output = usize; let shr(self)(rhs: usize): usize }
extend usize: core.cmp.Eq(usize) { let eq(self: borrow(usize))(rhs: borrow(usize)): bool }
extend usize: core.cmp.PartialOrd(usize) { let partial_cmp(self: borrow(usize))(rhs: borrow(usize)): core.cmp.PartialOrdering }
extend usize: core.ops.assign.AddAssign(usize) { let add_assign(self: borrow(mut)(usize))(rhs: usize): () }
extend usize: core.ops.assign.SubAssign(usize) { let sub_assign(self: borrow(mut)(usize))(rhs: usize): () }
extend usize: core.ops.assign.MulAssign(usize) { let mul_assign(self: borrow(mut)(usize))(rhs: usize): () }
extend usize: core.ops.assign.DivAssign(usize) { let div_assign(self: borrow(mut)(usize))(rhs: usize): () }
extend usize: core.ops.assign.RemAssign(usize) { let rem_assign(self: borrow(mut)(usize))(rhs: usize): () }
extend usize: core.ops.assign.BitAndAssign(usize) { let bit_and_assign(self: borrow(mut)(usize))(rhs: usize): () }
extend usize: core.ops.assign.BitOrAssign(usize) { let bit_or_assign(self: borrow(mut)(usize))(rhs: usize): () }
extend usize: core.ops.assign.BitXorAssign(usize) { let bit_xor_assign(self: borrow(mut)(usize))(rhs: usize): () }
extend usize: core.ops.assign.ShlAssign(usize) { let shl_assign(self: borrow(mut)(usize))(rhs: usize): () }
extend usize: core.ops.assign.ShrAssign(usize) { let shr_assign(self: borrow(mut)(usize))(rhs: usize): () }

extend i32: core.marker.Copy {}
extend i32: core.ops.arith.Add(i32) { let Output = i32; let add(self)(rhs: i32): i32 }
extend i32: core.ops.arith.Sub(i32) { let Output = i32; let sub(self)(rhs: i32): i32 }
extend i32: core.ops.arith.Mul(i32) { let Output = i32; let mul(self)(rhs: i32): i32 }
extend i32: core.ops.arith.Div(i32) { let Output = i32; let div(self)(rhs: i32): i32 }
extend i32: core.ops.arith.Rem(i32) { let Output = i32; let rem(self)(rhs: i32): i32 }
extend i32: core.ops.arith.Neg { let Output = i32; let neg(self)(): i32 }
extend i32: core.ops.bit.BitAnd(i32) { let Output = i32; let bit_and(self)(rhs: i32): i32 }
extend i32: core.ops.bit.BitOr(i32) { let Output = i32; let bit_or(self)(rhs: i32): i32 }
extend i32: core.ops.bit.BitXor(i32) { let Output = i32; let bit_xor(self)(rhs: i32): i32 }
extend i32: core.ops.bit.Shl(i32) { let Output = i32; let shl(self)(rhs: i32): i32 }
extend i32: core.ops.bit.Shr(i32) { let Output = i32; let shr(self)(rhs: i32): i32 }
extend i32: core.cmp.Eq(i32) { let eq(self: borrow(i32))(rhs: borrow(i32)): bool }
extend i32: core.cmp.PartialOrd(i32) { let partial_cmp(self: borrow(i32))(rhs: borrow(i32)): core.cmp.PartialOrdering }
extend i32: core.ops.assign.AddAssign(i32) { let add_assign(self: borrow(mut)(i32))(rhs: i32): () }
extend i32: core.ops.assign.SubAssign(i32) { let sub_assign(self: borrow(mut)(i32))(rhs: i32): () }
extend i32: core.ops.assign.MulAssign(i32) { let mul_assign(self: borrow(mut)(i32))(rhs: i32): () }
extend i32: core.ops.assign.DivAssign(i32) { let div_assign(self: borrow(mut)(i32))(rhs: i32): () }
extend i32: core.ops.assign.RemAssign(i32) { let rem_assign(self: borrow(mut)(i32))(rhs: i32): () }
extend i32: core.ops.assign.BitAndAssign(i32) { let bit_and_assign(self: borrow(mut)(i32))(rhs: i32): () }
extend i32: core.ops.assign.BitOrAssign(i32) { let bit_or_assign(self: borrow(mut)(i32))(rhs: i32): () }
extend i32: core.ops.assign.BitXorAssign(i32) { let bit_xor_assign(self: borrow(mut)(i32))(rhs: i32): () }
extend i32: core.ops.assign.ShlAssign(i32) { let shl_assign(self: borrow(mut)(i32))(rhs: i32): () }
extend i32: core.ops.assign.ShrAssign(i32) { let shr_assign(self: borrow(mut)(i32))(rhs: i32): () }

extend i64: core.marker.Copy {}
extend i64: core.ops.arith.Add(i64) { let Output = i64; let add(self)(rhs: i64): i64 }
extend i64: core.ops.arith.Sub(i64) { let Output = i64; let sub(self)(rhs: i64): i64 }
extend i64: core.ops.arith.Mul(i64) { let Output = i64; let mul(self)(rhs: i64): i64 }
extend i64: core.ops.arith.Div(i64) { let Output = i64; let div(self)(rhs: i64): i64 }
extend i64: core.ops.arith.Rem(i64) { let Output = i64; let rem(self)(rhs: i64): i64 }
extend i64: core.ops.arith.Neg { let Output = i64; let neg(self)(): i64 }
extend i64: core.ops.bit.BitAnd(i64) { let Output = i64; let bit_and(self)(rhs: i64): i64 }
extend i64: core.ops.bit.BitOr(i64) { let Output = i64; let bit_or(self)(rhs: i64): i64 }
extend i64: core.ops.bit.BitXor(i64) { let Output = i64; let bit_xor(self)(rhs: i64): i64 }
extend i64: core.ops.bit.Shl(i64) { let Output = i64; let shl(self)(rhs: i64): i64 }
extend i64: core.ops.bit.Shr(i64) { let Output = i64; let shr(self)(rhs: i64): i64 }
extend i64: core.cmp.Eq(i64) { let eq(self: borrow(i64))(rhs: borrow(i64)): bool }
extend i64: core.cmp.PartialOrd(i64) { let partial_cmp(self: borrow(i64))(rhs: borrow(i64)): core.cmp.PartialOrdering }
extend i64: core.ops.assign.AddAssign(i64) { let add_assign(self: borrow(mut)(i64))(rhs: i64): () }
extend i64: core.ops.assign.SubAssign(i64) { let sub_assign(self: borrow(mut)(i64))(rhs: i64): () }
extend i64: core.ops.assign.MulAssign(i64) { let mul_assign(self: borrow(mut)(i64))(rhs: i64): () }
extend i64: core.ops.assign.DivAssign(i64) { let div_assign(self: borrow(mut)(i64))(rhs: i64): () }
extend i64: core.ops.assign.RemAssign(i64) { let rem_assign(self: borrow(mut)(i64))(rhs: i64): () }
extend i64: core.ops.assign.BitAndAssign(i64) { let bit_and_assign(self: borrow(mut)(i64))(rhs: i64): () }
extend i64: core.ops.assign.BitOrAssign(i64) { let bit_or_assign(self: borrow(mut)(i64))(rhs: i64): () }
extend i64: core.ops.assign.BitXorAssign(i64) { let bit_xor_assign(self: borrow(mut)(i64))(rhs: i64): () }
extend i64: core.ops.assign.ShlAssign(i64) { let shl_assign(self: borrow(mut)(i64))(rhs: i64): () }
extend i64: core.ops.assign.ShrAssign(i64) { let shr_assign(self: borrow(mut)(i64))(rhs: i64): () }

extend u32: core.marker.Copy {}
extend u32: core.ops.arith.Add(u32) { let Output = u32; let add(self)(rhs: u32): u32 }
extend u32: core.ops.arith.Sub(u32) { let Output = u32; let sub(self)(rhs: u32): u32 }
extend u32: core.ops.arith.Mul(u32) { let Output = u32; let mul(self)(rhs: u32): u32 }
extend u32: core.ops.arith.Div(u32) { let Output = u32; let div(self)(rhs: u32): u32 }
extend u32: core.ops.arith.Rem(u32) { let Output = u32; let rem(self)(rhs: u32): u32 }
extend u32: core.ops.bit.BitAnd(u32) { let Output = u32; let bit_and(self)(rhs: u32): u32 }
extend u32: core.ops.bit.BitOr(u32) { let Output = u32; let bit_or(self)(rhs: u32): u32 }
extend u32: core.ops.bit.BitXor(u32) { let Output = u32; let bit_xor(self)(rhs: u32): u32 }
extend u32: core.ops.bit.Shl(u32) { let Output = u32; let shl(self)(rhs: u32): u32 }
extend u32: core.ops.bit.Shr(u32) { let Output = u32; let shr(self)(rhs: u32): u32 }
extend u32: core.cmp.Eq(u32) { let eq(self: borrow(u32))(rhs: borrow(u32)): bool }
extend u32: core.cmp.PartialOrd(u32) { let partial_cmp(self: borrow(u32))(rhs: borrow(u32)): core.cmp.PartialOrdering }
extend u32: core.ops.assign.AddAssign(u32) { let add_assign(self: borrow(mut)(u32))(rhs: u32): () }
extend u32: core.ops.assign.SubAssign(u32) { let sub_assign(self: borrow(mut)(u32))(rhs: u32): () }
extend u32: core.ops.assign.MulAssign(u32) { let mul_assign(self: borrow(mut)(u32))(rhs: u32): () }
extend u32: core.ops.assign.DivAssign(u32) { let div_assign(self: borrow(mut)(u32))(rhs: u32): () }
extend u32: core.ops.assign.RemAssign(u32) { let rem_assign(self: borrow(mut)(u32))(rhs: u32): () }
extend u32: core.ops.assign.BitAndAssign(u32) { let bit_and_assign(self: borrow(mut)(u32))(rhs: u32): () }
extend u32: core.ops.assign.BitOrAssign(u32) { let bit_or_assign(self: borrow(mut)(u32))(rhs: u32): () }
extend u32: core.ops.assign.BitXorAssign(u32) { let bit_xor_assign(self: borrow(mut)(u32))(rhs: u32): () }
extend u32: core.ops.assign.ShlAssign(u32) { let shl_assign(self: borrow(mut)(u32))(rhs: u32): () }
extend u32: core.ops.assign.ShrAssign(u32) { let shr_assign(self: borrow(mut)(u32))(rhs: u32): () }

extend u64: core.marker.Copy {}
extend u64: core.ops.arith.Add(u64) { let Output = u64; let add(self)(rhs: u64): u64 }
extend u64: core.ops.arith.Sub(u64) { let Output = u64; let sub(self)(rhs: u64): u64 }
extend u64: core.ops.arith.Mul(u64) { let Output = u64; let mul(self)(rhs: u64): u64 }
extend u64: core.ops.arith.Div(u64) { let Output = u64; let div(self)(rhs: u64): u64 }
extend u64: core.ops.arith.Rem(u64) { let Output = u64; let rem(self)(rhs: u64): u64 }
extend u64: core.ops.bit.BitAnd(u64) { let Output = u64; let bit_and(self)(rhs: u64): u64 }
extend u64: core.ops.bit.BitOr(u64) { let Output = u64; let bit_or(self)(rhs: u64): u64 }
extend u64: core.ops.bit.BitXor(u64) { let Output = u64; let bit_xor(self)(rhs: u64): u64 }
extend u64: core.ops.bit.Shl(u64) { let Output = u64; let shl(self)(rhs: u64): u64 }
extend u64: core.ops.bit.Shr(u64) { let Output = u64; let shr(self)(rhs: u64): u64 }
extend u64: core.cmp.Eq(u64) { let eq(self: borrow(u64))(rhs: borrow(u64)): bool }
extend u64: core.cmp.PartialOrd(u64) { let partial_cmp(self: borrow(u64))(rhs: borrow(u64)): core.cmp.PartialOrdering }
extend u64: core.ops.assign.AddAssign(u64) { let add_assign(self: borrow(mut)(u64))(rhs: u64): () }
extend u64: core.ops.assign.SubAssign(u64) { let sub_assign(self: borrow(mut)(u64))(rhs: u64): () }
extend u64: core.ops.assign.MulAssign(u64) { let mul_assign(self: borrow(mut)(u64))(rhs: u64): () }
extend u64: core.ops.assign.DivAssign(u64) { let div_assign(self: borrow(mut)(u64))(rhs: u64): () }
extend u64: core.ops.assign.RemAssign(u64) { let rem_assign(self: borrow(mut)(u64))(rhs: u64): () }
extend u64: core.ops.assign.BitAndAssign(u64) { let bit_and_assign(self: borrow(mut)(u64))(rhs: u64): () }
extend u64: core.ops.assign.BitOrAssign(u64) { let bit_or_assign(self: borrow(mut)(u64))(rhs: u64): () }
extend u64: core.ops.assign.BitXorAssign(u64) { let bit_xor_assign(self: borrow(mut)(u64))(rhs: u64): () }
extend u64: core.ops.assign.ShlAssign(u64) { let shl_assign(self: borrow(mut)(u64))(rhs: u64): () }
extend u64: core.ops.assign.ShrAssign(u64) { let shr_assign(self: borrow(mut)(u64))(rhs: u64): () }
