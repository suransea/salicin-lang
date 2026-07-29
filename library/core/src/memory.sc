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

  /// Borrows the first element accepted by `predicate`.
  let find(comptime e: effects): with(e)(self: borrow(self))(move predicate: with(e)((borrow(t)): bool)): core.option(borrow(t)) = {
    let values = self.as_slice()
    values.find(predicate)
  }

  /// Returns the index of the first element accepted by `predicate`.
  let position(comptime e: effects): with(e)(self: borrow(self))(move predicate: with(e)((borrow(t)): bool)): core.option(u64) = {
    let values = self.as_slice()
    values.position(predicate)
  }

  /// Returns whether any element is accepted by `predicate`.
  let any(comptime e: effects): with(e)(self: borrow(self))(move predicate: with(e)((borrow(t)): bool)): bool = {
    let values = self.as_slice()
    values.any(predicate)
  }

  /// Returns whether every element is accepted by `predicate`.
  let all(comptime e: effects): with(e)(self: borrow(self))(move predicate: with(e)((borrow(t)): bool)): bool = {
    let values = self.as_slice()
    values.all(predicate)
  }

  /// Folds elements from left to right into `initial`.
  let fold(comptime e: effects, comptime accumulator: type): with(e)(self: borrow(self))(move initial: accumulator)(move combine: with(e)((accumulator, borrow(t)): accumulator)): accumulator = {
    let values = self.as_slice()
    values.fold(initial)(combine)
  }

  /// Swaps two elements, trapping before mutation when either index is invalid.
  let swap(self: borrow(mut)(self))(left: u64, right: u64): () = {
    let values = self.as_slice(mut)()
    values.swap(left, right)
  }

  /// Reverses all elements in place.
  let reverse(self: borrow(mut)(self))(): () = {
    let values = self.as_slice(mut)()
    values.reverse()
  }
}

/// Provides equality-based membership for fixed-size arrays.
extend(array(t)(l))
(requires: t is core.marker.copyable && t is core.cmp.eq(t)) {
  /// Returns whether this array contains an element equal to `needle`.
  let contains(self: borrow(self))(copy needle: t): bool = {
    let values = self.as_slice()
    values.contains(needle)
  }
}

/// Provides copy-based mutation for fixed-size arrays.
extend(array(t)(l))
(requires: t is core.marker.copyable) {
  /// Replaces every element with a copy of `value`.
  let fill(self: borrow(mut)(self))(copy value: t): () = {
    let values = self.as_slice(mut)()
    values.fill(value)
  }

  /// Copies an equally sized source slice into this array.
  let copy_from(self: borrow(mut)(self))(source: borrow(slice(t))): () = {
    let values = self.as_slice(mut)()
    values.copy_from(source)
  }

  /// Copies a source range within this array with overlap-safe semantics.
  let copy_within(self: borrow(mut)(self))
    (source_start: u64, source_end: u64, destination_start: u64): () = {
    let values = self.as_slice(mut)()
    values.copy_within(source_start, source_end, destination_start)
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

  /// Borrows the first element accepted by `predicate`.
  let find(comptime e: effects): with(e)(self: borrow(self))(move predicate: with(e)((borrow(t)): bool)): core.option(borrow(t)) = {
    let length = self.len()
    let mut index: u64 = 0
    while { index < length } {
      let item = self.at(index)
      if predicate(item) {
        return(self.get(index))
      }
      index = index + 1
    }
    core.option.none
  }

  /// Returns the index of the first element accepted by `predicate`.
  let position(comptime e: effects): with(e)(self: borrow(self))(move predicate: with(e)((borrow(t)): bool)): core.option(u64) = {
    let length = self.len()
    let mut index: u64 = 0
    while { index < length } {
      let item = self.at(index)
      if predicate(item) {
        return(core.option.some(index))
      }
      index = index + 1
    }
    core.option.none
  }

  /// Returns whether any element is accepted by `predicate`.
  let any(comptime e: effects): with(e)(self: borrow(self))(move predicate: with(e)((borrow(t)): bool)): bool = {
    let length = self.len()
    let mut index: u64 = 0
    while { index < length } {
      let item = self.at(index)
      if predicate(item) {
        return(true)
      }
      index = index + 1
    }
    false
  }

  /// Returns whether every element is accepted by `predicate`.
  let all(comptime e: effects): with(e)(self: borrow(self))(move predicate: with(e)((borrow(t)): bool)): bool = {
    let length = self.len()
    let mut index: u64 = 0
    while { index < length } {
      let item = self.at(index)
      if !predicate(item) {
        return(false)
      }
      index = index + 1
    }
    true
  }

  /// Folds elements from left to right into `initial`.
  let fold(comptime e: effects, comptime accumulator: type): with(e)(self: borrow(self))(move initial: accumulator)(move combine: with(e)((accumulator, borrow(t)): accumulator)): accumulator = {
    let length = self.len()
    let mut value = initial
    let mut index: u64 = 0
    while { index < length } {
      let item = self.at(index)
      value = combine(value, item)
      index = index + 1
    }
    value
  }

  /// Swaps two elements, trapping before mutation when either index is invalid.
  let swap(self: borrow(mut)(self))(left: u64, right: u64): () = {
    let length = self.len(mut)()
    if left >= length || right >= length {
      unsafe {
        raw_trap()
      }
    }
    if left != right {
      let values = unsafe {
        raw_slice_ptr(mut)(self)
      }
      let left_pointer = unsafe {
        raw_offset(values, left)
      }
      let right_pointer = unsafe {
        raw_offset(values, right)
      }
      let left_value = unsafe {
        raw_take(left_pointer)
      }
      let right_value = unsafe {
        raw_take(right_pointer)
      }
      unsafe {
        raw_init(left_pointer, right_value)
        raw_init(right_pointer, left_value)
      }
    }
  }

  /// Reverses all elements in place.
  let reverse(self: borrow(mut)(self))(): () = {
    let length = self.len(mut)()
    let mut left: u64 = 0
    while { left < length / 2 } {
      self.swap(left, length - 1 - left)
      left = left + 1
    }
  }
}

/// Provides equality-based membership for borrowed slices.
extend(slice(t))
(requires: t is core.marker.copyable && t is core.cmp.eq(t)) {
  /// Returns whether this slice contains an element equal to `needle`.
  let contains(self: borrow(self))(copy needle: t): bool = {
    let length = self.len()
    if length > 0 {
      let values = unsafe {
        raw_slice_ptr(self)
      }
      let mut index: u64 = 0
      while { index < length } {
        let item = unsafe {
          *raw_offset(values, index)
        }
        if item == needle {
          return(true)
        }
        index = index + 1
      }
    }
    false
  }
}

/// Provides copy-based mutation for borrowed contiguous sequences.
extend(slice(t))
(requires: t is core.marker.copyable) {
  /// Replaces every element with a copy of `value`.
  let fill(self: borrow(mut)(self))(copy value: t): () = {
    let length = self.len(mut)()
    if length > 0 {
      let values = unsafe {
        raw_slice_ptr(mut)(self)
      }
      let mut index: u64 = 0
      while { index < length } {
        unsafe {
          *raw_offset(values, index) = value
        }
        index = index + 1
      }
    }
  }

  /// Copies `source` into this slice, trapping before mutation on a length mismatch.
  let copy_from(self: borrow(mut)(self))(source: borrow(slice(t))): () = {
    let length = self.len(mut)()
    if source.len() != length {
      unsafe {
        raw_trap()
      }
    }
    if length > 0 {
      let source_values = unsafe {
        raw_slice_ptr(source)
      }
      let destination_values = unsafe {
        raw_slice_ptr(mut)(self)
      }
      let mut index: u64 = 0
      while { index < length } {
        unsafe {
          *raw_offset(destination_values, index) = *raw_offset(source_values, index)
        }
        index = index + 1
      }
    }
  }

  /// Copies `[source_start, source_end)` to `destination_start`.
  ///
  /// Overlap is supported. All bounds are validated before the first write.
  let copy_within(self: borrow(mut)(self))
    (source_start: u64, source_end: u64, destination_start: u64): () = {
    let length = self.len(mut)()
    if source_start > source_end || source_end > length {
      unsafe {
        raw_trap()
      }
    }
    let count = source_end - source_start
    if destination_start > length || count > length - destination_start {
      unsafe {
        raw_trap()
      }
    }
    if count > 0 {
      let values = unsafe {
        raw_slice_ptr(mut)(self)
      }
      if destination_start > source_start {
        let mut remaining = count
        while { remaining > 0 } {
          remaining = remaining - 1
          unsafe {
            *raw_offset(values, destination_start + remaining) =
              *raw_offset(values, source_start + remaining)
          }
        }
      } else {
        let mut offset: u64 = 0
        while { offset < count } {
          unsafe {
            *raw_offset(values, destination_start + offset) =
              *raw_offset(values, source_start + offset)
          }
          offset = offset + 1
        }
      }
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
  let offset: with(core.unsafe.unsafety)(self)(index: u64): ptr(a)(t) = {
    unsafe {
      raw_offset(self, index)
    }
  }
}

/// Provides operations that require mutable raw-pointer access.
extend(ptr(mut)(t)) {
  /// Initializes storage that is currently uninitialized.
  let init: with(core.unsafe.unsafety)(self)(value: t): () = {
    unsafe {
      raw_init(self, value)
    }
  }

  /// Moves a value out and leaves the storage uninitialized.
  let take: with(core.unsafe.unsafety)(self)(): t = {
    unsafe {
      raw_take(self)
    }
  }
}

/// Returns the target size of `T` in bytes.
pub let size_of(comptime t: type): u64 = builtin()

/// Returns the target alignment of `T` in bytes.
pub let align_of(comptime t: type): u64 = builtin()
