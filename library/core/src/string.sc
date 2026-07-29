let ptr = core.memory.ptr
let slice = core.memory.slice

/// Dynamically sized immutable UTF-8 text.
///
/// Safe code can only use this type through a shared borrow constructed by
/// checked UTF-8 validation or from an already-valid `string`.
pub let str: type = builtin()

let read_byte(value: borrow(u8)): u8 = { value }

let byte_at(bytes: borrow(slice(u8)), index: u64): u8 = {
  let value = bytes.at(index)
  read_byte(value)
}

let byte_u32(value: u8): u32 = {
  let mut source = value
  let mut output: u32 = 0
  while { source != 0 } {
    source = source - 1
    output = output + 1
  }
  output
}

let is_continuation(byte: u8): bool = {
  let low: u8 = 128
  let high: u8 = 191
  byte >= low && byte <= high
}

let text_equal(left: borrow(str), right: borrow(str)): bool = {
  let left_bytes = left.as_bytes()
  let right_bytes = right.as_bytes()
  let length = left_bytes.len()
  if length != right_bytes.len() {
    return(false)
  }
  let mut index: u64 = 0
  while { index < length } {
    if byte_at(left_bytes, index) != byte_at(right_bytes, index) {
      return(false)
    }
    index = index + 1
  }
  true
}

let text_compare(
  left: borrow(str),
  right: borrow(str),
): core.cmp.partial_ordering = {
  let left_bytes = left.as_bytes()
  let right_bytes = right.as_bytes()
  let shared_length = left_bytes.len().min(right_bytes.len())
  let mut index: u64 = 0
  while { index < shared_length } {
    let left_byte = byte_at(left_bytes, index)
    let right_byte = byte_at(right_bytes, index)
    if left_byte < right_byte {
      return(less)
    }
    if left_byte > right_byte {
      return(greater)
    }
    index = index + 1
  }
  if left_bytes.len() < right_bytes.len() {
    less
  } else if left_bytes.len() > right_bytes.len() {
    greater
  } else {
    equal
  }
}

let text_matches_at(
  value: borrow(str),
  needle: borrow(str),
  start: u64,
): bool = {
  let value_bytes = value.as_bytes()
  let needle_bytes = needle.as_bytes()
  if start > value_bytes.len() ||
    needle_bytes.len() > value_bytes.len() - start {
    return(false)
  }
  let mut index: u64 = 0
  while { index < needle_bytes.len() } {
    if byte_at(value_bytes, start + index) != byte_at(needle_bytes, index) {
      return(false)
    }
    index = index + 1
  }
  true
}

let text_find(value: borrow(str), needle: borrow(str)): core.option(u64) = {
  let value_length = value.len()
  let needle_length = needle.len()
  if needle_length == 0 {
    return(core.option.some(0))
  }
  if needle_length > value_length {
    return(core.option.none)
  }
  let last_start = value_length - needle_length
  let mut start: u64 = 0
  while { start <= last_start } {
    if value.is_char_boundary(start) &&
      text_matches_at(value, needle, start) {
      return(core.option.some(start))
    }
    start = start + 1
  }
  core.option.none
}

/// Validates the exact well-formed UTF-8 byte sequences from Unicode Table
/// 3-7. The implementation is allocation-free and reads each byte at most a
/// constant number of times.
let utf8_error_offset(bytes: borrow(slice(u8))): core.option(u64) = {
  let length = bytes.len()
  let mut index: u64 = 0
  let one: u64 = 1
  let two: u64 = 2
  let three: u64 = 3
  let four: u64 = 4
  let ascii_max: u8 = 127
  let two_min: u8 = 194
  let two_max: u8 = 223
  let e0: u8 = 224
  let e0_second_min: u8 = 160
  let continuation_min: u8 = 128
  let continuation_max: u8 = 191
  let e1: u8 = 225
  let ec: u8 = 236
  let ed: u8 = 237
  let ed_second_max: u8 = 159
  let ee: u8 = 238
  let ef: u8 = 239
  let f0: u8 = 240
  let f0_second_min: u8 = 144
  let f1: u8 = 241
  let f3: u8 = 243
  let f4: u8 = 244
  let f4_second_max: u8 = 143
  while { index < length } {
    let first: u8 = byte_at(bytes, index)
    if first <= ascii_max {
      index = index + one
    } else if first >= two_min && first <= two_max {
      if index + one >= length ||
        !is_continuation(byte_at(bytes, index + one)) {
        return(core.option.some(index))
      }
      index = index + two
    } else if first == e0 {
      if index + two >= length {
        return(core.option.some(index))
      }
      let second: u8 = byte_at(bytes, index + one)
      if second < e0_second_min || second > continuation_max ||
        !is_continuation(byte_at(bytes, index + two)) {
        return(core.option.some(index))
      }
      index = index + three
    } else if first >= e1 && first <= ec {
      if index + two >= length ||
        !is_continuation(byte_at(bytes, index + one)) ||
        !is_continuation(byte_at(bytes, index + two)) {
        return(core.option.some(index))
      }
      index = index + three
    } else if first == ed {
      if index + two >= length {
        return(core.option.some(index))
      }
      let second: u8 = byte_at(bytes, index + one)
      if second < continuation_min || second > ed_second_max ||
        !is_continuation(byte_at(bytes, index + two)) {
        return(core.option.some(index))
      }
      index = index + three
    } else if first >= ee && first <= ef {
      if index + two >= length ||
        !is_continuation(byte_at(bytes, index + one)) ||
        !is_continuation(byte_at(bytes, index + two)) {
        return(core.option.some(index))
      }
      index = index + three
    } else if first == f0 {
      if index + three >= length {
        return(core.option.some(index))
      }
      let second: u8 = byte_at(bytes, index + one)
      if second < f0_second_min || second > continuation_max ||
        !is_continuation(byte_at(bytes, index + two)) ||
        !is_continuation(byte_at(bytes, index + three)) {
        return(core.option.some(index))
      }
      index = index + four
    } else if first >= f1 && first <= f3 {
      if index + three >= length ||
        !is_continuation(byte_at(bytes, index + one)) ||
        !is_continuation(byte_at(bytes, index + two)) ||
        !is_continuation(byte_at(bytes, index + three)) {
        return(core.option.some(index))
      }
      index = index + four
    } else if first == f4 {
      if index + three >= length {
        return(core.option.some(index))
      }
      let second: u8 = byte_at(bytes, index + one)
      if second < continuation_min || second > f4_second_max ||
        !is_continuation(byte_at(bytes, index + two)) ||
        !is_continuation(byte_at(bytes, index + three)) {
        return(core.option.some(index))
      }
      index = index + four
    } else {
      return(core.option.some(index))
    }
  }
  core.option.none
}

let is_valid_utf8(bytes: borrow(slice(u8))): bool = {
  utf8_error_offset(bytes).is_none()
}

/// Copies UTF-8 bytes while retaining a shared loan on the source text.
pub let str_bytes = struct {
  value: borrow(str),
  next_index: u64,
}

/// Decodes Unicode scalars while retaining a shared loan on the source text.
pub let str_scalars = struct {
  value: borrow(str),
  next_index: u64,
}

extend(str_bytes, core.iter.iterator) {
  let item = core.iter.owned_item(u8)
  let next(comptime r: region)
    (self: borrow(mut)(r)(self))(): core.option(u8) = {
    let bytes = unsafe { raw_str_bytes(self.value) }
    if self.next_index == bytes.len() {
      core.option.none
    } else {
      let byte = byte_at(bytes, self.next_index)
      self.next_index = self.next_index + 1
      core.option.some(byte)
    }
  }
}

extend(str_bytes, core.iter.into_iterator) {
  let iter = str_bytes
  let into_iter(move self)(): str_bytes = { self }
}

extend(str_scalars, core.iter.iterator) {
  let item = core.iter.owned_item(unicode_scalar)
  let next(comptime r: region)
    (self: borrow(mut)(r)(self))(): core.option(unicode_scalar) = {
    let bytes = unsafe { raw_str_bytes(self.value) }
    if self.next_index == bytes.len() {
      return(core.option.none)
    }
    let first = byte_at(bytes, self.next_index)
    let width: u64 = if first <= 127 { 1 }
      else if first <= 223 { 2 }
      else if first <= 239 { 3 }
      else { 4 }
    let mut code = if width == 1 {
      byte_u32(first)
    } else if width == 2 {
      byte_u32(first & 31) << 6 |
        byte_u32(byte_at(bytes, self.next_index + 1) & 63)
    } else if width == 3 {
      byte_u32(first & 15) << 12 |
        byte_u32(byte_at(bytes, self.next_index + 1) & 63) << 6 |
        byte_u32(byte_at(bytes, self.next_index + 2) & 63)
    } else {
      byte_u32(first & 7) << 18 |
        byte_u32(byte_at(bytes, self.next_index + 1) & 63) << 12 |
        byte_u32(byte_at(bytes, self.next_index + 2) & 63) << 6 |
        byte_u32(byte_at(bytes, self.next_index + 3) & 63)
    }
    self.next_index = self.next_index + width
    core.option.some(unicode_scalar { value: code })
  }
}

extend(str_scalars, core.iter.into_iterator) {
  let iter = str_scalars
  let into_iter(move self)(): str_scalars = { self }
}

extend(str) {
  /// Validates borrowed bytes and returns a text view with the same region.
  let from_utf8(comptime r: region)
    (bytes: borrow(r)(slice(u8))): core.option(borrow(r)(str)) = {
    if is_valid_utf8(bytes) {
      core.option.some(unsafe { raw_str(bytes) })
    } else {
      core.option.none
    }
  }

  /// Returns the length of the valid UTF-8 prefix, or `none` when all bytes
  /// are valid. For a truncated sequence this is the leading-byte offset.
  let first_invalid_utf8(bytes: borrow(slice(u8))): core.option(u64) = {
    utf8_error_offset(bytes)
  }

  /// Returns the number of UTF-8 bytes.
  let len(self: borrow(self))(): u64 = {
    let bytes = unsafe { raw_str_bytes(self) }
    bytes.len()
  }

  /// Returns whether this view contains no bytes.
  let is_empty(self: borrow(self))(): bool = { self.len() == 0 }

  /// Exposes the validated bytes with the same source region.
  let as_bytes(comptime r: region)
    (self: borrow(r)(self))(): borrow(r)(slice(u8)) = {
    unsafe {
      raw_str_bytes(self)
    }
  }

  /// Returns whether `index` is a UTF-8 code-point boundary.
  let is_char_boundary(self: borrow(self))(index: u64): bool = {
    let length = self.len()
    if index == 0 || index == length {
      true
    } else if index > length {
      false
    } else {
      let bytes = self.as_bytes()
      !is_continuation(byte_at(bytes, index))
    }
  }

  /// Returns the byte range as a view when both endpoints are UTF-8
  /// boundaries. The returned view retains this view's source region.
  let get(comptime r: region)
    (self: borrow(r)(self))
    (start: u64, end: u64): core.option(borrow(r)(str)) = {
    if start > end ||
      !self.is_char_boundary(start) ||
      !self.is_char_boundary(end) {
      core.option.none
    } else {
      core.option.some(unsafe { raw_subview(self, start, end - start) })
    }
  }

  /// Returns whether this text begins with `prefix`.
  let starts_with(self: borrow(self))(prefix: borrow(str)): bool = {
    text_matches_at(self, prefix, 0)
  }

  /// Returns whether this text ends with `suffix`.
  let ends_with(self: borrow(self))(suffix: borrow(str)): bool = {
    if suffix.len() > self.len() {
      false
    } else {
      text_matches_at(self, suffix, self.len() - suffix.len())
    }
  }

  /// Returns the first matching UTF-8 byte offset.
  ///
  /// The empty needle matches at zero. Every returned offset is a scalar
  /// boundary in this text.
  let find(self: borrow(self))(needle: borrow(str)): core.option(u64) = {
    text_find(self, needle)
  }

  /// Returns whether `needle` occurs in this text.
  let contains(self: borrow(self))(needle: borrow(str)): bool = {
    self.find(needle).is_some()
  }

  /// Iterates over copied UTF-8 bytes while retaining this source loan.
  let bytes(self: borrow(self))(): str_bytes = {
    str_bytes { value: self, next_index: 0 }
  }

  /// Iterates over copied Unicode scalar values while retaining this source
  /// loan.
  let scalars(self: borrow(self))(): str_scalars = {
    str_scalars { value: self, next_index: 0 }
  }

  /// Returns the number of Unicode scalar values.
  let scalar_count(self: borrow(self))(): u64 = {
    let mut values = self.scalars()
    let mut count: u64 = 0
    while { values.next().is_some() } {
      count = count + 1
    }
    count
  }

  /// Returns the scalar at `index`, or `none` when out of range.
  let scalar_at(self: borrow(self))(index: u64): core.option(unicode_scalar) = {
    let mut values = self.scalars()
    let mut current: u64 = 0
    while { current < index } {
      if values.next().is_none() {
        return(core.option.none)
      }
      current = current + 1
    }
    values.next()
  }
}

extend(str, core.cmp.eq(str)) {
  let eq(self: borrow(self))(other: borrow(str)): bool = {
    text_equal(self, other)
  }
}

extend(str, core.cmp.partial_ord(str)) {
  let partial_cmp(
    self: borrow(self),
  )(other: borrow(str)): core.cmp.partial_ordering = {
    text_compare(self, other)
  }
}

/// One Unicode scalar value: any code point except U+D800..U+DFFF.
///
/// The private representation prevents safe source from constructing a
/// surrogate or a value above U+10FFFF.
pub let unicode_scalar = struct {
  value: u32,
}

extend(unicode_scalar, core.marker.copyable) {}

extend(unicode_scalar, core.cmp.eq(unicode_scalar)) {
  /// Compares scalar values by numeric code point.
  let eq(self: borrow(self))(other: borrow(unicode_scalar)): bool = {
    self.value == other.value
  }
}

extend(unicode_scalar) {
  /// Constructs a scalar from its numeric value, rejecting surrogates and
  /// values above the Unicode codespace.
  let from_u32(value: u32): core.option(unicode_scalar) = {
    if value > 1114111 || value >= 55296 && value <= 57343 {
      core.option.none
    } else {
      core.option.some(unicode_scalar { value: value })
    }
  }

  /// Returns this scalar's numeric code point.
  let to_u32(self: borrow(self))(): u32 = { self.value }

  /// Returns the number of bytes in this scalar's canonical UTF-8 encoding.
  let len_utf8(self: borrow(self))(): u64 = {
    if self.value <= 127 {
      1
    } else if self.value <= 2047 {
      2
    } else if self.value <= 65535 {
      3
    } else {
      4
    }
  }
}

/// Immutable UTF-8 text shared by compile-time evaluation and runtime code.
///
/// `capacity` is representation metadata. A value whose capacity is zero
/// borrows static literal storage; non-zero capacity is reserved for owned
/// storage supplied by the allocation package.
pub let string = struct {
  data: ptr(mut)(u8),
  length: u64,
  storage_capacity: u64,
}

let string_allocate(capacity: u64): ptr(mut)(u8) = {
  unsafe {
    raw_alloc(u8)(capacity, 1)
  }
}

let string_deallocate(data: ptr(mut)(u8), capacity: u64): () = {
  unsafe {
    raw_dealloc(u8)(data, capacity, 1)
  }
}

let string_reserve(value: borrow(mut)(string), additional: u64): () = {
  if additional > 18446744073709551615 - value.length {
    unsafe {
      raw_trap()
    }
  }
  let required = value.length + additional
  if required > value.storage_capacity {
    let mut new_capacity: u64 = if value.storage_capacity == 0 {
      1
    } else {
      value.storage_capacity
    }
    while { new_capacity < required } {
      new_capacity = if new_capacity > 9223372036854775807 {
        required
      } else {
        new_capacity * 2
      }
    }
    let new_data = string_allocate(new_capacity)
    let mut index: u64 = 0
    while { index < value.length } {
      let byte = unsafe {
        *raw_offset(value.data, index)
      }
      unsafe {
        raw_init(raw_offset(new_data, index), byte)
      }
      index = index + 1
    }
    if value.storage_capacity != 0 {
      string_deallocate(value.data, value.storage_capacity)
    }
    value.data = new_data
    value.storage_capacity = new_capacity
  }
}

let string_copy_from_str(comptime r: region)(value: borrow(r)(str)): string = {
  let length = value.len()
  if length == 0 {
    ""
  } else {
    let data = string_allocate(length)
    let bytes = value.as_bytes()
    let mut index: u64 = 0
    while { index < length } {
      unsafe {
        raw_init(raw_offset(data, index), byte_at(bytes, index))
      }
      index = index + 1
    }
    string { data: data, length: length, storage_capacity: length }
  }
}

let scalar_byte(value: u32): u8 = {
  let mut remaining = value
  let mut byte: u8 = 0
  while { remaining != 0 } {
    byte = byte + 1
    remaining = remaining - 1
  }
  byte
}

let string_push_byte(value: borrow(mut)(string), byte: u8): () = {
  unsafe {
    raw_init(raw_offset(value.data, value.length), byte)
  }
  value.length = value.length + 1
}

let string_is_char_boundary(value: borrow(string), index: u64): bool = {
  if index == 0 || index == value.length {
    true
  } else if index > value.length {
    false
  } else {
    let byte = unsafe {
      *raw_offset(value.data, index)
    }
    !is_continuation(byte)
  }
}

/// Rebuilds unique string ownership from validated initialized byte storage.
pub let string_from_raw_parts: with(core.unsafe.unsafety)(
  data: ptr(mut)(u8),
  length: u64,
  capacity: u64,
): string = {
  string { data: data, length: length, storage_capacity: capacity }
}

/// Consumes a string into its representation. Capacity zero means borrowed
/// static storage and must be copied before constructing an owned byte vector.
pub let string_into_raw_parts: with(core.unsafe.unsafety)(
  move value: string,
): (ptr(mut)(u8), u64, u64) = {
  let parts = (value.data, value.length, value.storage_capacity)
  forget(value)
  parts
}

extend(string, core.literal.string_literal) {
  let output = string

  let from_string_literal(comptime length: usize)
    (move utf8: array(u8)(length)): output = builtin()
}

extend(string) {
  /// Creates an empty string without allocating.
  let new(): string = { "" }

  /// Creates an empty string with space for at least `capacity` bytes.
  let with_capacity(capacity: u64): string = {
    if capacity == 0 {
      ""
    } else {
      string { data: string_allocate(capacity), length: 0, storage_capacity: capacity }
    }
  }

  /// Copies borrowed validated text into a new owning string.
  let from_str(comptime r: region)(value: borrow(r)(str)): string = {
    string_copy_from_str(value)
  }

  /// Encodes one Unicode scalar into a new owning string.
  let from_unicode_scalar(value: unicode_scalar): string = {
    let mut text = string.with_capacity(value.len_utf8())
    text.push(value)
    text
  }

  /// Returns the number of UTF-8 bytes.
  let len_bytes(self: borrow(self))(): u64 = { self.length }

  /// Returns whether the string contains no bytes.
  let is_empty(self: borrow(self))(): bool = { self.length == 0 }

  /// Returns the number of bytes that fit without reallocating.
  let capacity(self: borrow(self))(): u64 = { self.storage_capacity }

  /// Ensures space for at least `additional` more UTF-8 bytes.
  let reserve(self: borrow(mut)(self))(additional: u64): () = {
    string_reserve(self, additional)
  }

  /// Borrows this string as immutable validated UTF-8.
  let as_str(comptime r: region)
    (self: borrow(r)(self))(): borrow(r)(str) = {
    unsafe {
      let bytes = raw_slice(self.data, self.length, borrow(self))
      raw_str(bytes)
    }
  }

  /// Appends borrowed validated UTF-8 text.
  let push_str(comptime r: region)
    (self: borrow(mut)(self))
    (value: borrow(r)(str)): () = {
    let bytes = value.as_bytes()
    let length = bytes.len()
    string_reserve(self, length)
    let mut index: u64 = 0
    while { index < length } {
      string_push_byte(self, byte_at(bytes, index))
      index = index + 1
    }
  }

  /// Appends one Unicode scalar in its canonical UTF-8 encoding.
  let push(self: borrow(mut)(self))(value: unicode_scalar): () = {
    let code = value.to_u32()
    string_reserve(self, value.len_utf8())
    if code <= 127 {
      string_push_byte(self, scalar_byte(code))
    } else if code <= 2047 {
      string_push_byte(self, scalar_byte(192 | code >> 6))
      string_push_byte(self, scalar_byte(128 | code & 63))
    } else if code <= 65535 {
      string_push_byte(self, scalar_byte(224 | code >> 12))
      string_push_byte(self, scalar_byte(128 | code >> 6 & 63))
      string_push_byte(self, scalar_byte(128 | code & 63))
    } else {
      string_push_byte(self, scalar_byte(240 | code >> 18))
      string_push_byte(self, scalar_byte(128 | code >> 12 & 63))
      string_push_byte(self, scalar_byte(128 | code >> 6 & 63))
      string_push_byte(self, scalar_byte(128 | code & 63))
    }
  }

  /// Truncates to `new_length` bytes when it is a UTF-8 boundary.
  ///
  /// Returns false without mutation for an out-of-bounds or interior offset.
  let truncate(self: borrow(mut)(self))(new_length: u64): bool = {
    if !string_is_char_boundary(self, new_length) {
      false
    } else {
      self.length = new_length
      true
    }
  }

  /// Removes all text while retaining owned capacity.
  let clear(self: borrow(mut)(self))(): () = {
    self.length = 0
  }

  /// Copies a checked UTF-8 byte range into a new owning string.
  let substring(self: borrow(self))(start: u64, end: u64): core.option(string) = {
    let view = self.as_str()
    match view.get(start, end)
      { some(part) -> core.option.some(string_copy_from_str(part)) }
      { none -> core.option.none }
  }

  /// Returns whether this string begins with `prefix`.
  let starts_with(self: borrow(self))(prefix: borrow(str)): bool = {
    let view = self.as_str()
    view.starts_with(prefix)
  }

  /// Returns whether this string ends with `suffix`.
  let ends_with(self: borrow(self))(suffix: borrow(str)): bool = {
    let view = self.as_str()
    view.ends_with(suffix)
  }

  /// Returns the first matching UTF-8 byte offset.
  let find(self: borrow(self))(needle: borrow(str)): core.option(u64) = {
    let view = self.as_str()
    view.find(needle)
  }

  /// Returns whether `needle` occurs in this string.
  let contains(self: borrow(self))(needle: borrow(str)): bool = {
    self.find(needle).is_some()
  }
}

extend(string, core.cmp.eq(string)) {
  let eq(self: borrow(self))(other: borrow(string)): bool = {
    let left = self.as_str()
    let right = other.as_str()
    text_equal(left, right)
  }
}

extend(string, core.cmp.partial_ord(string)) {
  let partial_cmp(
    self: borrow(self),
  )(other: borrow(string)): core.cmp.partial_ordering = {
    let left = self.as_str()
    let right = other.as_str()
    text_compare(left, right)
  }
}

extend(string, core.marker.droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    if self.storage_capacity != 0 {
      string_deallocate(self.data, self.storage_capacity)
    }
  }
}
