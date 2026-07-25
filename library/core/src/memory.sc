// Primitive pointer and layout contracts. The source declarations define the
// public identities and signatures; the compiler supplies representation,
// authority checks, and target-specific lowering after validating this module.
/// Fixed-size array type with compile-time element type and length.
pub let Array(T: type)
  (L: usize): type

/// Dynamically sized contiguous sequence viewed through a borrow.
pub let Slice(T: type): type

/// Provides operations on a borrowed contiguous sequence.
extend(T: type) Slice(T) {
  /// Returns the number of elements in this slice.
  let len(A: access = shared)(self: borrow(A)(Self))(): u64 = {
    unsafe {
      raw_slice_len(self)
    }
  }

  /// Borrows the element at `index`, trapping if `index` is out of bounds.
  let at(A: access = shared)(self: borrow(A)(Self))(index: u64): borrow(A)(T) = {
    unsafe {
      raw_slice_at(A)(self, index)
    }
  }
}

/// Raw pointer type with access `A` and pointee `T`.
pub let Ptr(A: access = shared)
  (T: type): type

/// Forms a raw pointer from a borrow with the same access.
pub let Ptr(A: access = shared)
  (T: type)
  (value: borrow(A)(T)): Ptr(A)(T)

/// Provides operations shared by raw pointers at either access.
extend(A: access, T: type) Ptr(A)(T) {
  /// Returns the pointer `index` elements after this pointer.
  let offset(self)(index: u64): Ptr(A)(T) with(core.effect.Unsafe) = {
    unsafe {
      raw_offset(self, index)
    }
  }
}

/// Provides operations that require mutable raw-pointer access.
extend(T: type) Ptr(mut)(T) {
  /// Initializes storage that is currently uninitialized.
  let init(self)(value: T): () with(core.effect.Unsafe) = {
    unsafe {
      raw_init(self, value)
    }
  }

  /// Moves a value out and leaves the storage uninitialized.
  let take(self)(): T with(core.effect.Unsafe) = {
    unsafe {
      raw_take(self)
    }
  }
}

/// Returns the target size of `T` in bytes.
pub let size_of(T: type): u64

/// Returns the target alignment of `T` in bytes.
pub let align_of(T: type): u64
