// Primitive pointer and layout contracts. The source declarations define the
// public identities and signatures; the compiler supplies representation,
// authority checks, and target-specific lowering after validating this module.
let index = core.ops.index

/// Fixed-size array type with compile-time element type and length.
pub let array(comptime t: type)
  (comptime l: usize): type = builtin()

/// Dynamically sized contiguous sequence viewed through a borrow.
pub let slice(comptime t: type): type = builtin()

/// Routes fixed-size array brackets through the source-defined indexing protocol.
extend(array(t)(l), index(usize)) {
  let output = t
  let index(comptime a: access)
    (self: borrow(a)(self))
    (key: usize): borrow(a)(t) = builtin()
}

/// Provides operations on a borrowed contiguous sequence.
extend(slice(t)) {
  /// Returns the number of elements in this slice.
  let len(comptime a: access = shared)(self: borrow(a)(self))(): u64 = {
    unsafe {
      raw_slice_len(self)
    }
  }

  /// Borrows the element at `index`, trapping if `index` is out of bounds.
  let at(comptime a: access = shared)(self: borrow(a)(self))(index: u64): borrow(a)(t) = {
    unsafe {
      raw_slice_at(a)(self, index)
    }
  }
}

/// Routes bracket access through the source-defined indexing protocol.
extend(slice(t), index(u64)) {
  let output = t
  let index(comptime a: access)
    (self: borrow(a)(self))
    (key: u64): borrow(a)(t) = {
    self.at(a)(key)
  }
}

/// Raw pointer type with access `A` and pointee `T`.
pub let ptr(comptime a: access = shared)
  (comptime t: type): type = builtin()

/// Forms a raw pointer from a borrow with the same access.
pub let ptr(comptime a: access = shared)
  (comptime t: type)
  (value: borrow(a)(t)): ptr(a)(t) = builtin()

/// Provides operations shared by raw pointers at either access.
extend(ptr(a)(t)) {
  /// Returns the pointer `index` elements after this pointer.
  let offset(self)(index: u64): ptr(a)(t) with(core.unsafe.unsafety) = {
    unsafe {
      raw_offset(self, index)
    }
  }
}

/// Provides operations that require mutable raw-pointer access.
extend(ptr(mut)(t)) {
  /// Initializes storage that is currently uninitialized.
  let init(self)(value: t): () with(core.unsafe.unsafety) = {
    unsafe {
      raw_init(self, value)
    }
  }

  /// Moves a value out and leaves the storage uninitialized.
  let take(self)(): t with(core.unsafe.unsafety) = {
    unsafe {
      raw_take(self)
    }
  }
}

/// Returns the target size of `T` in bytes.
pub let size_of(comptime t: type): u64 = builtin()

/// Returns the target alignment of `T` in bytes.
pub let align_of(comptime t: type): u64 = builtin()
