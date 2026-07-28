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

/// Provides access operations shared with slices and vectors.
extend(array(t)(l)) {
  /// Borrows all elements as a slice with the same source region.
  let as_slice(comptime a: access = shared)
    (self: borrow(a)(self))(): borrow(a)(slice(t)) = {
    unsafe {
      raw_array_slice(a)(self)
    }
  }

  /// Returns the number of elements.
  let len(self: borrow(self))(): u64 = {
    let values = self.as_slice()
    values.len()
  }

  /// Returns whether this array contains no elements.
  let is_empty(self: borrow(self))(): bool = { self.len() == 0 }

  /// Borrows the element at `index`, or returns `none` when out of bounds.
  let get(comptime a: access = shared)
    (self: borrow(a)(self))
    (index: u64): core.option(borrow(a)(t)) = {
    let values = self.as_slice(a)()
    values.get(a)(index)
  }

  /// Borrows the element at `index`, trapping when out of bounds.
  let at(comptime a: access = shared)
    (self: borrow(a)(self))
    (index: u64): borrow(a)(t) = {
    let values = self.as_slice(a)()
    values.at(a)(index)
  }

  /// Borrows the first element, or returns `none` when empty.
  let first(comptime a: access = shared)
    (self: borrow(a)(self))(): core.option(borrow(a)(t)) = {
    self.get(a)(0)
  }

  /// Borrows the last element, or returns `none` when empty.
  let last(comptime a: access = shared)
    (self: borrow(a)(self))(): core.option(borrow(a)(t)) = {
    let values = self.as_slice(a)()
    values.last(a)()
  }
}

/// Provides operations on a borrowed contiguous sequence.
extend(slice(t)) {
  /// Returns the number of elements in this slice.
  let len(comptime a: access = shared)(self: borrow(a)(self))(): u64 = {
    unsafe {
      raw_slice_len(self)
    }
  }

  /// Returns whether this slice contains no elements.
  let is_empty(comptime a: access = shared)(self: borrow(a)(self))(): bool = {
    self.len(a)() == 0
  }

  /// Borrows the element at `index`, or returns `none` when out of bounds.
  let get(comptime a: access = shared)
    (self: borrow(a)(self))
    (index: u64): core.option(borrow(a)(t)) = {
    if index >= self.len(a)() {
      core.option.none
    } else {
      core.option.some(unsafe {
        raw_slice_at(a)(self, index)
      })
    }
  }

  /// Borrows the element at `index`, trapping if `index` is out of bounds.
  let at(comptime a: access = shared)
    (self: borrow(a)(self))
    (index: u64): borrow(a)(t) = {
    unsafe {
      raw_slice_at(a)(self, index)
    }
  }

  /// Borrows the first element, or returns `none` when empty.
  let first(comptime a: access = shared)
    (self: borrow(a)(self))(): core.option(borrow(a)(t)) = {
    self.get(a)(0)
  }

  /// Borrows the last element, or returns `none` when empty.
  let last(comptime a: access = shared)
    (self: borrow(a)(self))(): core.option(borrow(a)(t)) = {
    let length = self.len(a)()
    if length == 0 {
      core.option.none
    } else {
      self.get(a)(length - 1)
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
