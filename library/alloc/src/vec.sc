use core.Option

/// Growable contiguous heap allocation for values of type `T`.
pub let Vec(T: type) = struct {
  /// Pointer to the start of the allocated storage.
  pointer: MutPtr(T),
  /// Number of initialized elements.
  length: u64,
  /// Number of elements that fit in the allocated storage.
  storage_capacity: u64,
}

/// Computes the byte size needed to store `capacity` elements of `T`.
let vec_layout_size(T: type)(capacity: u64): u64 = {
  let element_size = size_of(T)
  if element_size != 0 && capacity > 18446744073709551615 / element_size {
    unsafe {
      raw_trap()
    }
  }
  capacity * element_size
}

/// Allocates raw storage for `capacity` elements of `T`.
let vec_allocate(T: type)(capacity: u64): MutPtr(T) = {
  unsafe {
    raw_alloc(T)(vec_layout_size(T: T)(capacity), align_of(T))
  }
}

/// Deallocates raw vector storage previously allocated for `capacity` elements.
let vec_deallocate(T: type)(pointer: MutPtr(T), capacity: u64): () = {
  unsafe {
    raw_dealloc(T)(pointer, vec_layout_size(T: T)(capacity), align_of(T))
  }
}

/// Creates an empty vector with zero capacity.
pub let vec_new(T: type)(): Vec(T) = {
  Vec(T) { pointer: vec_allocate(T: T)(0), length: 0, storage_capacity: 0 }
}

/// Creates an empty vector with storage for `capacity` elements.
pub let vec_with_capacity(T: type)(capacity: u64): Vec(T) = {
  Vec(T) { pointer: vec_allocate(T: T)(capacity), length: 0, storage_capacity: capacity }
}

/// Returns the number of initialized elements in `values`.
pub let vec_len(T: type)(values: borrow(Vec(T))): u64 = { values.length }

/// Returns the number of elements that fit without reallocating.
pub let vec_capacity(T: type)(values: borrow(Vec(T))): u64 = { values.storage_capacity }

/// Borrows the element at `index`, trapping if `index` is out of bounds.
pub let vec_at(A: access, R: region, T: type)
  (values: borrow(A)(R)(Vec(T)))(index: u64): borrow(A)(R)(T) = {
  if index >= values.length {
    unsafe {
      raw_trap()
    }
  }
  unsafe {
    raw_borrow(A)(raw_offset(values.pointer, index), borrow(A)(values))
  }
}

/// Ensures that `values` can accept at least `additional` more elements.
pub let vec_reserve(T: type)(values: borrow(mut)(Vec(T)))(additional: u64): () = {
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
    while new_capacity < required_capacity {
      new_capacity = if new_capacity > 9223372036854775807 {
        required_capacity
      } else {
        new_capacity * 2
      }
    }
    let new_pointer = vec_allocate(T: T)(new_capacity)
    let mut index: u64 = 0
    while index < values.length {
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
pub let vec_push(T: type)(values: borrow(mut)(Vec(T)))(value: T): () = {
  vec_reserve(values)(1)
  unsafe {
    raw_init(raw_offset(values.pointer, values.length), value)
  }
  values.length = values.length + 1
}

/// Replaces the element at `index` and returns the previous element.
pub let vec_replace(T: type)(values: borrow(mut)(Vec(T)))(index: u64)(value: T): T = {
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
pub let vec_pop(T: type)(values: borrow(mut)(Vec(T))): Option(T) = {
  if values.length == 0 {
    Option(T).None
  } else {
    values.length = values.length - 1
    let value = unsafe {
      raw_take(raw_offset(values.pointer, values.length))
    }
    Option(T).Some(value)
  }
}

/// Drops elements from the end until the vector length is at most `new_length`.
pub let vec_truncate(T: type)(values: borrow(mut)(Vec(T)))(new_length: u64): () = {
  while values.length > new_length {
    values.length = values.length - 1
    let item = unsafe {
      raw_take(raw_offset(values.pointer, values.length))
    }
  }
}

/// Removes all elements from `values`.
pub let vec_clear(T: type)(values: borrow(mut)(Vec(T))): () = { vec_truncate(values)(0) }

/// Returns whether `values` has no initialized elements.
pub let vec_is_empty(T: type)(values: borrow(Vec(T))): bool = { values.length == 0 }

/// Removes the element at `index` by moving the last element into its slot.
pub let vec_swap_remove(T: type)(values: borrow(mut)(Vec(T)))(index: u64): T = {
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
pub let vec_swap(T: type)(values: borrow(mut)(Vec(T)))(left: u64, right: u64): () = {
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
pub let vec_reverse(T: type)(values: borrow(mut)(Vec(T))): () = {
  let mut left: u64 = 0
  while left < values.length / 2 {
    let right = values.length - 1 - left
    vec_swap(values)(left, right)
    left = left + 1
  }
}

/// Inserts `value` at `index`, shifting later elements right.
pub let vec_insert(T: type)(values: borrow(mut)(Vec(T)))(index: u64)(value: T): () = {
  if index > values.length {
    unsafe {
      raw_trap()
    }
  }
  vec_reserve(values)(1)
  let mut move_index = values.length
  while move_index > index {
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
pub let vec_remove(T: type)(values: borrow(mut)(Vec(T)))(index: u64): T = {
  if index >= values.length {
    unsafe {
      raw_trap()
    }
  }
  let removed = unsafe {
    raw_take(raw_offset(values.pointer, index))
  }
  let mut move_index = index
  while move_index + 1 < values.length {
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
pub let vec_append(T: type)(values: borrow(mut)(Vec(T)))(other: borrow(mut)(Vec(T))): () = {
  let start = values.length
  let moved = other.length
  vec_reserve(values)(moved)
  let mut index: u64 = 0
  while index < moved {
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
pub let vec_shrink_to_fit(T: type)(values: borrow(mut)(Vec(T))): () = {
  if values.length != values.storage_capacity {
    let new_pointer = vec_allocate(T: T)(values.length)
    let mut index: u64 = 0
    while index < values.length {
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
pub let vec_read(T: type)(values: borrow(Vec(T)))(index: u64): T
where T: Copy = {
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
pub let vec_write(T: type)(values: borrow(mut)(Vec(T)))(index: u64)(copy value: T): ()
where T: Copy = {
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
extend(T: type) Vec(T) {
  /// Creates an empty vector with zero capacity.
  let new(): Vec(T) = { vec_new() }
  /// Creates an empty vector with storage for `capacity` elements.
  let with_capacity(capacity: u64): Vec(T) = { vec_with_capacity(capacity) }
  /// Returns the number of initialized elements.
  let len(self: borrow(Self))(): u64 = { vec_len(self) }
  /// Returns the number of elements that fit without reallocating.
  let capacity(self: borrow(Self))(): u64 = { vec_capacity(self) }
  /// Borrows the element at `index`, trapping if it is out of bounds.
  let at(A: access)(self: borrow(A)(Self))(index: u64): borrow(A)(T) = {
    if index >= self.length {
      unsafe {
        raw_trap()
      }
    }
    unsafe {
      raw_borrow(A)(raw_offset(self.pointer, index), borrow(A)(self))
    }
  }
  /// Ensures capacity for at least `additional` more elements.
  let reserve(self: borrow(mut)(Self))(additional: u64): () = { vec_reserve(self)(additional) }
  /// Appends `value` to the end of this vector.
  let push(self: borrow(mut)(Self))(value: T): () = { vec_push(self)(value) }
  /// Replaces the element at `index` and returns the previous element.
  let replace(self: borrow(mut)(Self))(index: u64)(value: T): T = { vec_replace(self)(index)(value) }
  /// Removes and returns the last element, or `None` if empty.
  let pop(self: borrow(mut)(Self))(): Option(T) = { vec_pop(self) }
  /// Drops elements from the end until the length is at most `new_length`.
  let truncate(self: borrow(mut)(Self))(new_length: u64): () = { vec_truncate(self)(new_length) }
  /// Removes all elements from this vector.
  let clear(self: borrow(mut)(Self))(): () = { vec_clear(self) }
  /// Returns whether this vector has no initialized elements.
  let is_empty(self: borrow(Self))(): bool = { vec_is_empty(self) }
  /// Removes an element by replacing it with the last element.
  let swap_remove(self: borrow(mut)(Self))(index: u64): T = { vec_swap_remove(self)(index) }
  /// Swaps the elements at `left` and `right`.
  let swap(self: borrow(mut)(Self))(left: u64, right: u64): () = { vec_swap(self)(left: left, right: right) }
  /// Reverses the initialized elements in place.
  let reverse(self: borrow(mut)(Self))(): () = { vec_reverse(self) }
  /// Inserts `value` at `index`, shifting later elements right.
  let insert(self: borrow(mut)(Self))(index: u64)(value: T): () = { vec_insert(self)(index)(value) }
  /// Removes and returns the element at `index`, shifting later elements left.
  let remove(self: borrow(mut)(Self))(index: u64): T = { vec_remove(self)(index) }
  /// Moves all elements from `other` onto the end of this vector.
  let append(self: borrow(mut)(Self))(other: borrow(mut)(Vec(T))): () = { vec_append(self)(other) }
  /// Reallocates storage so capacity matches the current length.
  let shrink_to_fit(self: borrow(mut)(Self))(): () = { vec_shrink_to_fit(self) }
}

/// Provides copy-only indexed read and write operations.
extend(T: type) Vec(T)
where T: Copy {
  /// Copies the element at `index` out of this vector.
  let read(self: borrow(Self))(index: u64): T = { vec_read(self)(index) }
  /// Copies `value` into the element slot at `index`.
  let write(self: borrow(mut)(Self))(index: u64)(copy value: T): () = { vec_write(self)(index)(value) }
}

/// Drops initialized elements and releases vector storage.
extend(T: type) Vec(T): Drop {
  /// Drops all initialized elements and deallocates storage.
  let drop(self: borrow(mut)(Self))(): () = {
    let mut index: u64 = 0
    while index < self.length {
      let item = unsafe {
        raw_take(raw_offset(self.pointer, index))
      }
      index = index + 1
    }
    vec_deallocate(self.pointer, self.storage_capacity)
  }
}
