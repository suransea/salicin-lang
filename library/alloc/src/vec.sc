use core.Option

pub let Vec(T: type) = struct { pointer: MutPtr(T),
  length: u64,
  storage_capacity: u64, }

let vec_layout_size(T: type)(capacity: u64): u64 = {
  let element_size = size_of(T)
  if element_size != 0 && capacity > 18446744073709551615 / element_size {
    unsafe {
      raw_trap()
    }
  }
  capacity * element_size
}

let vec_allocate(T: type)(capacity: u64): MutPtr(T) = {
  unsafe {
    raw_alloc(T)(vec_layout_size(T: T)(capacity), align_of(T))
  }
}

let vec_deallocate(T: type)(pointer: MutPtr(T), capacity: u64): () = {
  unsafe {
    raw_dealloc(T)(pointer, vec_layout_size(T: T)(capacity), align_of(T))
  }
}

pub let vec_new(T: type)(): Vec(T) = {
  Vec(T) { pointer: vec_allocate(T: T)(0), length: 0, storage_capacity: 0 }
}

pub let vec_with_capacity(T: type)(capacity: u64): Vec(T) = {
  Vec(T) { pointer: vec_allocate(T: T)(capacity), length: 0, storage_capacity: capacity }
}

pub let vec_len(T: type)(borrow values: Vec(T)): u64 = { values.length }

pub let vec_capacity(T: type)(borrow values: Vec(T)): u64 = { values.storage_capacity }

pub let vec_at(A: access, 'a: region, T: type)
  (borrow(A, 'a) values: Vec(T))(index: u64): borrow(A, 'a) T = {
  if index >= values.length {
    unsafe {
      raw_trap()
    }
  }
  unsafe {
    raw_borrow(A)(raw_offset(values.pointer, index), borrow(A) values)
  }
}

pub let vec_reserve(T: type)(borrow(mut) values: Vec(T))(additional: u64): () = {
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

pub let vec_push(T: type)(borrow(mut) values: Vec(T))(value: T): () = {
  vec_reserve(values)(1)
  unsafe {
    raw_init(raw_offset(values.pointer, values.length), value)
  }
  values.length = values.length + 1
}

pub let vec_replace(T: type)(borrow(mut) values: Vec(T))(index: u64)(value: T): T = {
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

pub let vec_pop(T: type)(borrow(mut) values: Vec(T)): Option(T) = {
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

pub let vec_truncate(T: type)(borrow(mut) values: Vec(T))(new_length: u64): () = {
  while values.length > new_length {
    values.length = values.length - 1
    let item = unsafe {
      raw_take(raw_offset(values.pointer, values.length))
    }
  }
}

pub let vec_clear(T: type)(borrow(mut) values: Vec(T)): () = { vec_truncate(values)(0) }

pub let vec_is_empty(T: type)(borrow values: Vec(T)): bool = { values.length == 0 }

pub let vec_swap_remove(T: type)(borrow(mut) values: Vec(T))(index: u64): T = {
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

pub let vec_swap(T: type)(borrow(mut) values: Vec(T))(left: u64, right: u64): () = {
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

pub let vec_reverse(T: type)(borrow(mut) values: Vec(T)): () = {
  let mut left: u64 = 0
  while left < values.length / 2 {
    let right = values.length - 1 - left
    vec_swap(values)(left, right)
    left = left + 1
  }
}

pub let vec_insert(T: type)(borrow(mut) values: Vec(T))(index: u64)(value: T): () = {
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

pub let vec_remove(T: type)(borrow(mut) values: Vec(T))(index: u64): T = {
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

pub let vec_append(T: type)(borrow(mut) values: Vec(T))(borrow(mut) other: Vec(T)): () = {
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

pub let vec_shrink_to_fit(T: type)(borrow(mut) values: Vec(T)): () = {
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

pub let vec_read(T: type)(borrow values: Vec(T))(index: u64): T
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

pub let vec_write(T: type)(borrow(mut) values: Vec(T))(index: u64)(copy value: T): ()
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

extend(T: type) Vec(T) {
  let new(): Vec(T) = { vec_new() }
  let with_capacity(capacity: u64): Vec(T) = { vec_with_capacity(capacity) }
  let len(borrow self)(): u64 = { vec_len(self) }
  let capacity(borrow self)(): u64 = { vec_capacity(self) }
  let at(A: access)(borrow(A) self)(index: u64): borrow(A) T = {
    if index >= self.length {
      unsafe {
        raw_trap()
      }
    }
    unsafe {
      raw_borrow(A)(raw_offset(self.pointer, index), borrow(A) self)
    }
  }
  let reserve(borrow(mut) self)(additional: u64): () = { vec_reserve(self)(additional) }
  let push(borrow(mut) self)(value: T): () = { vec_push(self)(value) }
  let replace(borrow(mut) self)(index: u64)(value: T): T = { vec_replace(self)(index)(value) }
  let pop(borrow(mut) self)(): Option(T) = { vec_pop(self) }
  let truncate(borrow(mut) self)(new_length: u64): () = { vec_truncate(self)(new_length) }
  let clear(borrow(mut) self)(): () = { vec_clear(self) }
  let is_empty(borrow self)(): bool = { vec_is_empty(self) }
  let swap_remove(borrow(mut) self)(index: u64): T = { vec_swap_remove(self)(index) }
  let swap(borrow(mut) self)(left: u64, right: u64): () = { vec_swap(self)(left: left, right: right) }
  let reverse(borrow(mut) self)(): () = { vec_reverse(self) }
  let insert(borrow(mut) self)(index: u64)(value: T): () = { vec_insert(self)(index)(value) }
  let remove(borrow(mut) self)(index: u64): T = { vec_remove(self)(index) }
  let append(borrow(mut) self)(borrow(mut) other: Vec(T)): () = { vec_append(self)(other) }
  let shrink_to_fit(borrow(mut) self)(): () = { vec_shrink_to_fit(self) }
}

extend(T: type) Vec(T)
where T: Copy {
  let read(borrow self)(index: u64): T = { vec_read(self)(index) }
  let write(borrow(mut) self)(index: u64)(copy value: T): () = { vec_write(self)(index)(value) }
}

extend(T: type) Vec(T): Drop {
  let drop(borrow(mut) self)(): () = {
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
