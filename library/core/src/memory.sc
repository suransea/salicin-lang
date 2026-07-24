// Primitive pointer and layout contracts. The source declarations define the
// public identities and signatures; the compiler supplies representation,
// authority checks, and target-specific lowering after validating this module.
/// Fixed-size array type with compile-time element type and length.
pub let Array(T: type)
  (L: usize): type

/// Shared raw pointer type.
pub let Ptr(T: type): type

/// Mutable raw pointer type.
pub let MutPtr(T: type): type

/// Forms a shared raw pointer from an explicit shared borrow.
pub let Ptr(T: type)
  (value: borrow(T)): Ptr(T)

/// Forms a mutable raw pointer from an explicit mutable borrow.
pub let MutPtr(T: type)
  (value: borrow(mut)(T)): MutPtr(T)

/// Returns the target size of `T` in bytes.
pub let size_of(T: type): u64

/// Returns the target alignment of `T` in bytes.
pub let align_of(T: type): u64
