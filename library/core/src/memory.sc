// Primitive pointer and layout contracts. The source declarations define the
// public identities and signatures; the compiler supplies representation,
// authority checks, and target-specific lowering after validating this module.
/// Fixed-size array type with compile-time element type and length.
pub let Array(T: type)
  (L: usize): type

/// Raw pointer type with access `A` and pointee `T`.
pub let Ptr(A: access = shared)
  (T: type): type

/// Forms a raw pointer from a borrow with the same access.
pub let Ptr(A: access = shared)
  (T: type)
  (value: borrow(A)(T)): Ptr(A)(T)

/// Returns the target size of `T` in bytes.
pub let size_of(T: type): u64

/// Returns the target alignment of `T` in bytes.
pub let align_of(T: type): u64
