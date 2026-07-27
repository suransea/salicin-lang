let option = core.option
let slice = core.slice
let index = core.ops.index
let iterator = core.iter.iterator
let into_iterator = core.iter.into_iterator
let owned_item = core.iter.owned_item

/// Growable contiguous heap allocation for values of type `T`.
pub let vec(comptime t: type) = struct {
  /// Pointer to the start of the allocated storage.
  pointer: ptr(mut)(t),
  /// Number of initialized elements.
  length: u64,
  /// Number of elements that fit in the allocated storage.
  storage_capacity: u64,
}

/// Computes the byte size needed to store `capacity` elements of `T`.
let vec_layout_size(comptime t: type)(capacity: u64): u64 = {
  let element_size = size_of(t)
  if element_size != 0 && capacity > 18446744073709551615 / element_size {
    unsafe {
      raw_trap()
    }
  }
  capacity * element_size
}

/// Allocates raw storage for `capacity` elements of `T`.
let vec_allocate(comptime t: type)(capacity: u64): ptr(mut)(t) = {
  unsafe {
    raw_alloc(t)(vec_layout_size(t: t)(capacity), align_of(t))
  }
}

/// Deallocates raw vector storage previously allocated for `capacity` elements.
let vec_deallocate(comptime t: type)(pointer: ptr(mut)(t), capacity: u64): () = {
  unsafe {
    raw_dealloc(t)(pointer, vec_layout_size(t: t)(capacity), align_of(t))
  }
}

/// Creates an empty vector with zero capacity.
let vec_new(comptime t: type)(): vec(t) = {
  vec(t) { pointer: vec_allocate(t: t)(0), length: 0, storage_capacity: 0 }
}

/// Creates an empty vector with storage for `capacity` elements.
let vec_with_capacity(comptime t: type)(capacity: u64): vec(t) = {
  vec(t) { pointer: vec_allocate(t: t)(capacity), length: 0, storage_capacity: capacity }
}

/// Returns the number of initialized elements in `values`.
let vec_len(comptime t: type)(values: borrow(vec(t))): u64 = { values.length }

/// Returns the number of elements that fit without reallocating.
let vec_capacity(comptime t: type)(values: borrow(vec(t))): u64 = { values.storage_capacity }

/// Borrows the element at `index`, trapping if `index` is out of bounds.
let vec_at(comptime a: access, comptime r: region, comptime t: type)
  (values: borrow(a)(r)(vec(t)))(index: u64): borrow(a)(r)(t) = {
  if index >= values.length {
    unsafe {
      raw_trap()
    }
  }
  unsafe {
    raw_borrow(a)(raw_offset(values.pointer, index), borrow(a)(values))
  }
}

/// Ensures that `values` can accept at least `additional` more elements.
let vec_reserve(comptime t: type)(values: borrow(mut)(vec(t)))(additional: u64): () = {
  if additional > 18446744073709551615 - values.length {
    unsafe {
      raw_trap()
    }
  }
  let required_capacity = values.length + additional
  if required_capacity > values.storage_capacity {
    let mut new_capacity: u64 = if values.storage_capacity == 0 {
      1
    } else {
      values.storage_capacity
    }
    while { new_capacity < required_capacity } {
      new_capacity = if new_capacity > 9223372036854775807 {
        required_capacity
      } else {
        new_capacity * 2
      }
    }
    let new_pointer = vec_allocate(t)(new_capacity)
    let mut index: u64 = 0
    while { index < values.length } {
      let item = unsafe {
        raw_take(raw_offset(values.pointer, index))
      }
      unsafe {
        raw_init(raw_offset(new_pointer, index), item)
      }
      index = index + 1
    }
    vec_deallocate(values.pointer, values.storage_capacity)
    values.pointer = new_pointer
    values.storage_capacity = new_capacity
  }
}

/// Appends `value` to the end of `values`.
let vec_push(comptime t: type)(values: borrow(mut)(vec(t)))(value: t): () = {
  vec_reserve(values)(1)
  unsafe {
    raw_init(raw_offset(values.pointer, values.length), value)
  }
  values.length = values.length + 1
}

/// Replaces the element at `index` and returns the previous element.
let vec_replace(comptime t: type)(values: borrow(mut)(vec(t)))(index: u64)(value: t): t = {
  if index >= values.length {
    unsafe {
      raw_trap()
    }
  }
  let pointer = unsafe {
    raw_offset(values.pointer, index)
  }
  let previous = unsafe {
    raw_take(pointer)
  }
  unsafe {
    raw_init(pointer, value)
  }
  previous
}

/// Removes and returns the last element, or `None` if the vector is empty.
let vec_pop(comptime t: type)(values: borrow(mut)(vec(t))): option(t) = {
  if values.length == 0 {
    option(t).none
  } else {
    values.length = values.length - 1
    let value = unsafe {
      raw_take(raw_offset(values.pointer, values.length))
    }
    option(t).some(value)
  }
}

/// Drops elements from the end until the vector length is at most `new_length`.
let vec_truncate(comptime t: type)(values: borrow(mut)(vec(t)))(new_length: u64): () = {
  while { values.length > new_length } {
    values.length = values.length - 1
    let item = unsafe {
      raw_take(raw_offset(values.pointer, values.length))
    }
  }
}

/// Removes all elements from `values`.
let vec_clear(comptime t: type)(values: borrow(mut)(vec(t))): () = { vec_truncate(values)(0) }

/// Returns whether `values` has no initialized elements.
let vec_is_empty(comptime t: type)(values: borrow(vec(t))): bool = { values.length == 0 }

/// Removes the element at `index` by moving the last element into its slot.
let vec_swap_remove(comptime t: type)(values: borrow(mut)(vec(t)))(index: u64): t = {
  if index >= values.length {
    unsafe {
      raw_trap()
    }
  }
  let last_index = values.length - 1
  let removed = unsafe {
    raw_take(raw_offset(values.pointer, index))
  }
  if index != last_index {
    let last = unsafe {
      raw_take(raw_offset(values.pointer, last_index))
    }
    unsafe {
      raw_init(raw_offset(values.pointer, index), last)
    }
  }
  values.length = last_index
  removed
}

/// Swaps the elements at `left` and `right`.
let vec_swap(comptime t: type)(values: borrow(mut)(vec(t)))(left: u64, right: u64): () = {
  if left >= values.length || right >= values.length {
    unsafe {
      raw_trap()
    }
  }
  if left != right {
    let left_value = unsafe {
      raw_take(raw_offset(values.pointer, left))
    }
    let right_value = unsafe {
      raw_take(raw_offset(values.pointer, right))
    }
    unsafe {
      raw_init(raw_offset(values.pointer, left), right_value)
      raw_init(raw_offset(values.pointer, right), left_value)
    }
  }
}

/// Reverses the order of initialized elements in place.
let vec_reverse(comptime t: type)(values: borrow(mut)(vec(t))): () = {
  let mut left: u64 = 0
  while { left < values.length / 2 } {
    let right = values.length - 1 - left
    vec_swap(values)(left, right)
    left = left + 1
  }
}

/// Inserts `value` at `index`, shifting later elements right.
let vec_insert(comptime t: type)(values: borrow(mut)(vec(t)))(index: u64)(value: t): () = {
  if index > values.length {
    unsafe {
      raw_trap()
    }
  }
  vec_reserve(values)(1)
  let mut move_index = values.length
  while { move_index > index } {
    let previous_index = move_index - 1
    let item = unsafe {
      raw_take(raw_offset(values.pointer, previous_index))
    }
    unsafe {
      raw_init(raw_offset(values.pointer, move_index), item)
    }
    move_index = previous_index
  }
  unsafe {
    raw_init(raw_offset(values.pointer, index), value)
  }
  values.length = values.length + 1
}

/// Removes and returns the element at `index`, shifting later elements left.
let vec_remove(comptime t: type)(values: borrow(mut)(vec(t)))(index: u64): t = {
  if index >= values.length {
    unsafe {
      raw_trap()
    }
  }
  let removed = unsafe {
    raw_take(raw_offset(values.pointer, index))
  }
  let mut move_index = index
  while { move_index + 1 < values.length } {
    let next_index = move_index + 1
    let item = unsafe {
      raw_take(raw_offset(values.pointer, next_index))
    }
    unsafe {
      raw_init(raw_offset(values.pointer, move_index), item)
    }
    move_index = next_index
  }
  values.length = values.length - 1
  removed
}

/// Moves all elements from `other` onto the end of `values`.
let vec_append(comptime t: type)(values: borrow(mut)(vec(t)))(other: borrow(mut)(vec(t))): () = {
  let start = values.length
  let moved = other.length
  vec_reserve(values)(moved)
  let mut index: u64 = 0
  while { index < moved } {
    let item = unsafe {
      raw_take(raw_offset(other.pointer, index))
    }
    unsafe {
      raw_init(raw_offset(values.pointer, start + index), item)
    }
    index = index + 1
  }
  values.length = start + moved
  other.length = 0
}

/// Reallocates storage so capacity matches the current length.
let vec_shrink_to_fit(comptime t: type)(values: borrow(mut)(vec(t))): () = {
  if values.length != values.storage_capacity {
    let new_pointer = vec_allocate(t)(values.length)
    let mut index: u64 = 0
    while { index < values.length } {
      let item = unsafe {
        raw_take(raw_offset(values.pointer, index))
      }
      unsafe {
        raw_init(raw_offset(new_pointer, index), item)
      }
      index = index + 1
    }
    vec_deallocate(values.pointer, values.storage_capacity)
    values.pointer = new_pointer
    values.storage_capacity = values.length
  }
}

/// Copies the element at `index` out of `values`.
let vec_read(comptime t: type)(values: borrow(vec(t)))(index: u64): t
where t: copyable = {
  if index >= values.length {
    unsafe {
      raw_trap()
    }
  }
  unsafe {
    *raw_offset(values.pointer, index)
  }
}

/// Copies `value` into the element slot at `index`.
let vec_write(comptime t: type)(values: borrow(mut)(vec(t)))(index: u64)(copy value: t): ()
where t: copyable = {
  if index >= values.length {
    unsafe {
      raw_trap()
    }
  }
  unsafe {
    *raw_offset(values.pointer, index) = value
  }
}

/// Provides inherent vector constructors and mutation operations.
extend(vec(t)) {
  /// Creates an empty vector with zero capacity.
  let new(): vec(t) = { vec_new() }
  /// Creates an empty vector with storage for `capacity` elements.
  let with_capacity(capacity: u64): vec(t) = { vec_with_capacity(capacity) }
  /// Returns the number of initialized elements.
  let len(self: borrow(self))(): u64 = { vec_len(self) }
  /// Borrows all initialized elements as a slice.
  let as_slice(comptime a: access)(self: borrow(a)(self))(): borrow(a)(slice(t)) = {
    unsafe {
      raw_slice(a)(self.pointer, self.length, borrow(a)(self))
    }
  }
  /// Returns the number of elements that fit without reallocating.
  let capacity(self: borrow(self))(): u64 = { vec_capacity(self) }
  /// Borrows the element at `index`, trapping if it is out of bounds.
  let at(comptime a: access)(self: borrow(a)(self))(index: u64): borrow(a)(t) = {
    if index >= self.length {
      unsafe {
        raw_trap()
      }
    }
    unsafe {
      raw_borrow(a)(raw_offset(self.pointer, index), borrow(a)(self))
    }
  }
  /// Ensures capacity for at least `additional` more elements.
  let reserve(self: borrow(mut)(self))(additional: u64): () = { vec_reserve(self)(additional) }
  /// Appends `value` to the end of this vector.
  let push(self: borrow(mut)(self))(value: t): () = { vec_push(self)(value) }
  /// Replaces the element at `index` and returns the previous element.
  let replace(self: borrow(mut)(self))(index: u64)(value: t): t = { vec_replace(self)(index)(value) }
  /// Removes and returns the last element, or `None` if empty.
  let pop(self: borrow(mut)(self))(): option(t) = { vec_pop(self) }
  /// Drops elements from the end until the length is at most `new_length`.
  let truncate(self: borrow(mut)(self))(new_length: u64): () = { vec_truncate(self)(new_length) }
  /// Removes all elements from this vector.
  let clear(self: borrow(mut)(self))(): () = { vec_clear(self) }
  /// Returns whether this vector has no initialized elements.
  let is_empty(self: borrow(self))(): bool = { vec_is_empty(self) }
  /// Removes an element by replacing it with the last element.
  let swap_remove(self: borrow(mut)(self))(index: u64): t = { vec_swap_remove(self)(index) }
  /// Swaps the elements at `left` and `right`.
  let swap(self: borrow(mut)(self))(left: u64, right: u64): () = { vec_swap(self)(left: left, right: right) }
  /// Reverses the initialized elements in place.
  let reverse(self: borrow(mut)(self))(): () = { vec_reverse(self) }
  /// Inserts `value` at `index`, shifting later elements right.
  let insert(self: borrow(mut)(self))(index: u64)(value: t): () = { vec_insert(self)(index)(value) }
  /// Removes and returns the element at `index`, shifting later elements left.
  let remove(self: borrow(mut)(self))(index: u64): t = { vec_remove(self)(index) }
  /// Moves all elements from `other` onto the end of this vector.
  let append(self: borrow(mut)(self))(other: borrow(mut)(vec(t))): () = { vec_append(self)(other) }
  /// Replaces this vector with an empty one and returns its previous allocation.
  let take(self: borrow(mut)(self))(): vec(t) = {
    let previous = vec(t) { pointer: self.pointer, length: self.length, storage_capacity: self.storage_capacity }
    self.pointer = vec_allocate(0)
    self.length = 0
    self.storage_capacity = 0
    previous
  }
  /// Reallocates storage so capacity matches the current length.
  let shrink_to_fit(self: borrow(mut)(self))(): () = { vec_shrink_to_fit(self) }
}

/// Provides copy-only indexed read and write operations.
extend(vec(t))
where t: copyable {
  /// Copies the element at `index` out of this vector.
  let read(self: borrow(self))(index: u64): t = { vec_read(self)(index) }
  /// Copies `value` into the element slot at `index`.
  let write(self: borrow(mut)(self))(index: u64)(copy value: t): () = { vec_write(self)(index)(value) }
}

/// Owning iterator over a vector.
pub let vec_into_iter(comptime t: type) = struct {
  pointer: ptr(mut)(t),
  next_index: u64,
  length: u64,
  storage_capacity: u64,
}

/// Routes bracket access through the source-defined indexing protocol.
extend(vec(t), index(u64)) {
  let output = t
  let index(comptime a: access)
    (self: borrow(a)(self))
    (key: u64): borrow(a)(t) = {
    self.at(a)(key)
  }
}

/// Advances an owning vector iterator in source order.
extend(vec_into_iter(t), iterator) {
  let item = owned_item(t)
  let next(comptime r: region)(self: borrow(mut)(r)(self))(): option(t) = {
    if self.next_index == self.length {
      option(t).none
    } else {
      let value = unsafe {
        raw_take(raw_offset(self.pointer, self.next_index))
      }
      self.next_index = self.next_index + 1
      option(t).some(value)
    }
  }
}

/// Consumes a vector into an owning iterator.
extend(vec(t), into_iterator) {
  let iter = vec_into_iter(t)
  let into_iter(move self)(): vec_into_iter(t) = {
    let iterator = vec_into_iter(t) { pointer: self.pointer, next_index: 0, length: self.length, storage_capacity: self.storage_capacity }
    forget(self)
    iterator
  }
}

/// Drops elements not yet yielded and releases the transferred vector storage.
extend(vec_into_iter(t), droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    while { self.next_index < self.length } {
      let item = unsafe {
        raw_take(raw_offset(self.pointer, self.next_index))
      }
      self.next_index = self.next_index + 1
    }
    vec_deallocate(self.pointer, self.storage_capacity)
  }
}

/// Drops initialized elements and releases vector storage.
extend(vec(t), droppable) {
  /// Drops all initialized elements and deallocates storage.
  let drop(self: borrow(mut)(self))(): () = {
    let mut index: u64 = 0
    while { index < self.length } {
      let item = unsafe {
        raw_take(raw_offset(self.pointer, index))
      }
      index = index + 1
    }
    vec_deallocate(self.pointer, self.storage_capacity)
  }
}
