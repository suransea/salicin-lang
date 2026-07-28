let ptr = core.memory.ptr

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
  data: ptr(u8),
  length: u64,
  capacity: u64,
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

}
