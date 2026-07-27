let ptr = core.memory.ptr

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
