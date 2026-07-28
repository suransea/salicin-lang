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
}

extend(str, core.cmp.eq(str)) {
  let eq(self: borrow(self))(other: borrow(str)): bool = {
    text_equal(self, other)
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
  capacity: u64,
}

/// Rebuilds unique string ownership from validated initialized byte storage.
pub let string_from_raw_parts(
  data: ptr(mut)(u8),
  length: u64,
  capacity: u64,
): string with(core.unsafe.unsafety) = {
  string { data: data, length: length, capacity: capacity }
}

/// Consumes a string into its representation. Capacity zero means borrowed
/// static storage and must be copied before constructing an owned byte vector.
pub let string_into_raw_parts(
  move value: string,
): (ptr(mut)(u8), u64, u64) with(core.unsafe.unsafety) = {
  let parts = (value.data, value.length, value.capacity)
  forget(value)
  parts
}

extend(string, core.literal.string_literal) {
  let output = string

  let from_string_literal(comptime length: usize)
    (move utf8: array(u8)(length)): output = builtin()
}

extend(string) {
  /// Returns the number of UTF-8 bytes.
  let len_bytes(self: borrow(self))(): u64 = { self.length }

  /// Returns whether the string contains no bytes.
  let is_empty(self: borrow(self))(): bool = { self.length == 0 }

  /// Borrows this string as immutable validated UTF-8.
  let as_str(comptime r: region)
    (self: borrow(r)(self))(): borrow(r)(str) = {
    unsafe {
      let bytes = raw_slice(self.data, self.length, borrow(self))
      raw_str(bytes)
    }
  }
}

extend(string, core.cmp.eq(string)) {
  let eq(self: borrow(self))(other: borrow(string)): bool = {
    let left = self.as_str()
    let right = other.as_str()
    text_equal(left, right)
  }
}

extend(string, core.marker.droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    if self.capacity != 0 {
      unsafe {
        raw_dealloc(self.data, self.capacity, 1)
      }
    }
  }
}
