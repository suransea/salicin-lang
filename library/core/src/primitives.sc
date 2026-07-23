// Opaque primitive types. Their source identities and contracts participate in
// ordinary type and trait resolution; the compiler only supplies representation
// and intrinsic lowering after validating this core bundle.
pub let bool = type { false, true }
pub let i8 = type
pub let i16 = type
pub let i32 = type
pub let i64 = type
pub let i128 = type
pub let isize = type
pub let u8 = type
pub let u16 = type
pub let u32 = type
pub let u64 = type
pub let u128 = type
pub let usize = type

extend bool: core.marker.Copy {}
extend bool: core.ops.bit.Not {
  let Output = bool
  let not(move self)(): bool
}
extend bool: core.cmp.Eq(bool) {
  let eq(self: borrow(bool))(rhs: borrow(bool)): bool
}

// Integer contracts are deliberately stated in source. A bodyless method in
// this core-only section names a compiler intrinsic; it is not an implicit impl.
extend i32: core.marker.Copy {}
extend i32: core.ops.arith.Add(i32) { let Output = i32; let add(move self)(move rhs: i32): i32 }
extend i32: core.ops.arith.Sub(i32) { let Output = i32; let sub(move self)(move rhs: i32): i32 }
extend i32: core.ops.arith.Mul(i32) { let Output = i32; let mul(move self)(move rhs: i32): i32 }
extend i32: core.ops.arith.Div(i32) { let Output = i32; let div(move self)(move rhs: i32): i32 }
extend i32: core.ops.arith.Rem(i32) { let Output = i32; let rem(move self)(move rhs: i32): i32 }
extend i32: core.ops.arith.Neg { let Output = i32; let neg(move self)(): i32 }
extend i32: core.ops.bit.BitAnd(i32) { let Output = i32; let bit_and(move self)(move rhs: i32): i32 }
extend i32: core.ops.bit.BitOr(i32) { let Output = i32; let bit_or(move self)(move rhs: i32): i32 }
extend i32: core.ops.bit.BitXor(i32) { let Output = i32; let bit_xor(move self)(move rhs: i32): i32 }
extend i32: core.ops.bit.Shl(i32) { let Output = i32; let shl(move self)(move rhs: i32): i32 }
extend i32: core.ops.bit.Shr(i32) { let Output = i32; let shr(move self)(move rhs: i32): i32 }
extend i32: core.cmp.Eq(i32) { let eq(self: borrow(i32))(rhs: borrow(i32)): bool }
extend i32: core.cmp.PartialOrd(i32) { let partial_cmp(self: borrow(i32))(rhs: borrow(i32)): core.cmp.PartialOrdering }
extend i32: core.ops.assign.AddAssign(i32) { let add_assign(self: borrow(mut)(i32))(move rhs: i32): () }
extend i32: core.ops.assign.SubAssign(i32) { let sub_assign(self: borrow(mut)(i32))(move rhs: i32): () }
extend i32: core.ops.assign.MulAssign(i32) { let mul_assign(self: borrow(mut)(i32))(move rhs: i32): () }
extend i32: core.ops.assign.DivAssign(i32) { let div_assign(self: borrow(mut)(i32))(move rhs: i32): () }
extend i32: core.ops.assign.RemAssign(i32) { let rem_assign(self: borrow(mut)(i32))(move rhs: i32): () }
extend i32: core.ops.assign.BitAndAssign(i32) { let bit_and_assign(self: borrow(mut)(i32))(move rhs: i32): () }
extend i32: core.ops.assign.BitOrAssign(i32) { let bit_or_assign(self: borrow(mut)(i32))(move rhs: i32): () }
extend i32: core.ops.assign.BitXorAssign(i32) { let bit_xor_assign(self: borrow(mut)(i32))(move rhs: i32): () }
extend i32: core.ops.assign.ShlAssign(i32) { let shl_assign(self: borrow(mut)(i32))(move rhs: i32): () }
extend i32: core.ops.assign.ShrAssign(i32) { let shr_assign(self: borrow(mut)(i32))(move rhs: i32): () }

extend i64: core.marker.Copy {}
extend i64: core.ops.arith.Add(i64) { let Output = i64; let add(move self)(move rhs: i64): i64 }
extend i64: core.ops.arith.Sub(i64) { let Output = i64; let sub(move self)(move rhs: i64): i64 }
extend i64: core.ops.arith.Mul(i64) { let Output = i64; let mul(move self)(move rhs: i64): i64 }
extend i64: core.ops.arith.Div(i64) { let Output = i64; let div(move self)(move rhs: i64): i64 }
extend i64: core.ops.arith.Rem(i64) { let Output = i64; let rem(move self)(move rhs: i64): i64 }
extend i64: core.ops.arith.Neg { let Output = i64; let neg(move self)(): i64 }
extend i64: core.ops.bit.BitAnd(i64) { let Output = i64; let bit_and(move self)(move rhs: i64): i64 }
extend i64: core.ops.bit.BitOr(i64) { let Output = i64; let bit_or(move self)(move rhs: i64): i64 }
extend i64: core.ops.bit.BitXor(i64) { let Output = i64; let bit_xor(move self)(move rhs: i64): i64 }
extend i64: core.ops.bit.Shl(i64) { let Output = i64; let shl(move self)(move rhs: i64): i64 }
extend i64: core.ops.bit.Shr(i64) { let Output = i64; let shr(move self)(move rhs: i64): i64 }
extend i64: core.cmp.Eq(i64) { let eq(self: borrow(i64))(rhs: borrow(i64)): bool }
extend i64: core.cmp.PartialOrd(i64) { let partial_cmp(self: borrow(i64))(rhs: borrow(i64)): core.cmp.PartialOrdering }
extend i64: core.ops.assign.AddAssign(i64) { let add_assign(self: borrow(mut)(i64))(move rhs: i64): () }
extend i64: core.ops.assign.SubAssign(i64) { let sub_assign(self: borrow(mut)(i64))(move rhs: i64): () }
extend i64: core.ops.assign.MulAssign(i64) { let mul_assign(self: borrow(mut)(i64))(move rhs: i64): () }
extend i64: core.ops.assign.DivAssign(i64) { let div_assign(self: borrow(mut)(i64))(move rhs: i64): () }
extend i64: core.ops.assign.RemAssign(i64) { let rem_assign(self: borrow(mut)(i64))(move rhs: i64): () }
extend i64: core.ops.assign.BitAndAssign(i64) { let bit_and_assign(self: borrow(mut)(i64))(move rhs: i64): () }
extend i64: core.ops.assign.BitOrAssign(i64) { let bit_or_assign(self: borrow(mut)(i64))(move rhs: i64): () }
extend i64: core.ops.assign.BitXorAssign(i64) { let bit_xor_assign(self: borrow(mut)(i64))(move rhs: i64): () }
extend i64: core.ops.assign.ShlAssign(i64) { let shl_assign(self: borrow(mut)(i64))(move rhs: i64): () }
extend i64: core.ops.assign.ShrAssign(i64) { let shr_assign(self: borrow(mut)(i64))(move rhs: i64): () }

extend u32: core.marker.Copy {}
extend u32: core.ops.arith.Add(u32) { let Output = u32; let add(move self)(move rhs: u32): u32 }
extend u32: core.ops.arith.Sub(u32) { let Output = u32; let sub(move self)(move rhs: u32): u32 }
extend u32: core.ops.arith.Mul(u32) { let Output = u32; let mul(move self)(move rhs: u32): u32 }
extend u32: core.ops.arith.Div(u32) { let Output = u32; let div(move self)(move rhs: u32): u32 }
extend u32: core.ops.arith.Rem(u32) { let Output = u32; let rem(move self)(move rhs: u32): u32 }
extend u32: core.ops.bit.BitAnd(u32) { let Output = u32; let bit_and(move self)(move rhs: u32): u32 }
extend u32: core.ops.bit.BitOr(u32) { let Output = u32; let bit_or(move self)(move rhs: u32): u32 }
extend u32: core.ops.bit.BitXor(u32) { let Output = u32; let bit_xor(move self)(move rhs: u32): u32 }
extend u32: core.ops.bit.Shl(u32) { let Output = u32; let shl(move self)(move rhs: u32): u32 }
extend u32: core.ops.bit.Shr(u32) { let Output = u32; let shr(move self)(move rhs: u32): u32 }
extend u32: core.cmp.Eq(u32) { let eq(self: borrow(u32))(rhs: borrow(u32)): bool }
extend u32: core.cmp.PartialOrd(u32) { let partial_cmp(self: borrow(u32))(rhs: borrow(u32)): core.cmp.PartialOrdering }
extend u32: core.ops.assign.AddAssign(u32) { let add_assign(self: borrow(mut)(u32))(move rhs: u32): () }
extend u32: core.ops.assign.SubAssign(u32) { let sub_assign(self: borrow(mut)(u32))(move rhs: u32): () }
extend u32: core.ops.assign.MulAssign(u32) { let mul_assign(self: borrow(mut)(u32))(move rhs: u32): () }
extend u32: core.ops.assign.DivAssign(u32) { let div_assign(self: borrow(mut)(u32))(move rhs: u32): () }
extend u32: core.ops.assign.RemAssign(u32) { let rem_assign(self: borrow(mut)(u32))(move rhs: u32): () }
extend u32: core.ops.assign.BitAndAssign(u32) { let bit_and_assign(self: borrow(mut)(u32))(move rhs: u32): () }
extend u32: core.ops.assign.BitOrAssign(u32) { let bit_or_assign(self: borrow(mut)(u32))(move rhs: u32): () }
extend u32: core.ops.assign.BitXorAssign(u32) { let bit_xor_assign(self: borrow(mut)(u32))(move rhs: u32): () }
extend u32: core.ops.assign.ShlAssign(u32) { let shl_assign(self: borrow(mut)(u32))(move rhs: u32): () }
extend u32: core.ops.assign.ShrAssign(u32) { let shr_assign(self: borrow(mut)(u32))(move rhs: u32): () }

extend u64: core.marker.Copy {}
extend u64: core.ops.arith.Add(u64) { let Output = u64; let add(move self)(move rhs: u64): u64 }
extend u64: core.ops.arith.Sub(u64) { let Output = u64; let sub(move self)(move rhs: u64): u64 }
extend u64: core.ops.arith.Mul(u64) { let Output = u64; let mul(move self)(move rhs: u64): u64 }
extend u64: core.ops.arith.Div(u64) { let Output = u64; let div(move self)(move rhs: u64): u64 }
extend u64: core.ops.arith.Rem(u64) { let Output = u64; let rem(move self)(move rhs: u64): u64 }
extend u64: core.ops.bit.BitAnd(u64) { let Output = u64; let bit_and(move self)(move rhs: u64): u64 }
extend u64: core.ops.bit.BitOr(u64) { let Output = u64; let bit_or(move self)(move rhs: u64): u64 }
extend u64: core.ops.bit.BitXor(u64) { let Output = u64; let bit_xor(move self)(move rhs: u64): u64 }
extend u64: core.ops.bit.Shl(u64) { let Output = u64; let shl(move self)(move rhs: u64): u64 }
extend u64: core.ops.bit.Shr(u64) { let Output = u64; let shr(move self)(move rhs: u64): u64 }
extend u64: core.cmp.Eq(u64) { let eq(self: borrow(u64))(rhs: borrow(u64)): bool }
extend u64: core.cmp.PartialOrd(u64) { let partial_cmp(self: borrow(u64))(rhs: borrow(u64)): core.cmp.PartialOrdering }
extend u64: core.ops.assign.AddAssign(u64) { let add_assign(self: borrow(mut)(u64))(move rhs: u64): () }
extend u64: core.ops.assign.SubAssign(u64) { let sub_assign(self: borrow(mut)(u64))(move rhs: u64): () }
extend u64: core.ops.assign.MulAssign(u64) { let mul_assign(self: borrow(mut)(u64))(move rhs: u64): () }
extend u64: core.ops.assign.DivAssign(u64) { let div_assign(self: borrow(mut)(u64))(move rhs: u64): () }
extend u64: core.ops.assign.RemAssign(u64) { let rem_assign(self: borrow(mut)(u64))(move rhs: u64): () }
extend u64: core.ops.assign.BitAndAssign(u64) { let bit_and_assign(self: borrow(mut)(u64))(move rhs: u64): () }
extend u64: core.ops.assign.BitOrAssign(u64) { let bit_or_assign(self: borrow(mut)(u64))(move rhs: u64): () }
extend u64: core.ops.assign.BitXorAssign(u64) { let bit_xor_assign(self: borrow(mut)(u64))(move rhs: u64): () }
extend u64: core.ops.assign.ShlAssign(u64) { let shl_assign(self: borrow(mut)(u64))(move rhs: u64): () }
extend u64: core.ops.assign.ShrAssign(u64) { let shr_assign(self: borrow(mut)(u64))(move rhs: u64): () }
