let option = core.option
let result = core.result
let slice = core.slice
let vec = alloc.vec.vec

/// Owning, growable, well-formed UTF-8 text.
pub let string = struct {
  bytes: vec(u8),
}

/// Failed consuming UTF-8 conversion with ownership of the original bytes.
pub let from_utf8_error = struct {
  bytes: vec(u8),
  valid_prefix: u64,
}

/// Returns the start of the first invalid UTF-8 sequence.
let utf8_invalid_at(bytes: borrow(vec(u8))): option(u64) = {
  let mut index: u64 = 0
  while { index < bytes.len() } {
    let first = bytes.read(index)
    if first <= 127 {
      index = index + 1
    } else if first >= 194 && first <= 223 {
      if index + 1 >= bytes.len() {
        return(option(u64).some(index))
      }
      let second = bytes.read(index + 1)
      if second < 128 || second > 191 {
        return(option(u64).some(index))
      }
      index = index + 2
    } else if first >= 224 && first <= 239 {
      if index + 2 >= bytes.len() {
        return(option(u64).some(index))
      }
      let second = bytes.read(index + 1)
      let third = bytes.read(index + 2)
      if second < 128 || second > 191 || third < 128 || third > 191 {
        return(option(u64).some(index))
      }
      if first == 224 && second < 160 {
        return(option(u64).some(index))
      }
      if first == 237 && second > 159 {
        return(option(u64).some(index))
      }
      index = index + 3
    } else if first >= 240 && first <= 244 {
      if index + 3 >= bytes.len() {
        return(option(u64).some(index))
      }
      let second = bytes.read(index + 1)
      let third = bytes.read(index + 2)
      let fourth = bytes.read(index + 3)
      if second < 128 || second > 191 || third < 128 || third > 191 || fourth < 128 || fourth > 191 {
        return(option(u64).some(index))
      }
      if first == 240 && second < 144 {
        return(option(u64).some(index))
      }
      if first == 244 && second > 143 {
        return(option(u64).some(index))
      }
      index = index + 4
    } else {
      return(option(u64).some(index))
    }
  }
  option(u64).none
}

extend string {
  /// Creates an empty string with zero capacity.
  let new(): string = { string { bytes: vec(u8).new() } }

  /// Creates an empty string with capacity for `byte_capacity` bytes.
  let with_capacity(byte_capacity: u64): string = {
    string { bytes: vec(u8).with_capacity(byte_capacity) }
  }

  /// Validates and consumes an existing byte vector.
  let from_utf8(move bytes: vec(u8)): result(from_utf8_error)(string) = {
    match utf8_invalid_at(bytes)
      { some(valid_prefix) -> result.err(from_utf8_error { bytes: bytes, valid_prefix: valid_prefix }) }
      { none -> result.ok(string { bytes: bytes }) }
  }

  /// Consumes bytes known by the caller to contain well-formed UTF-8.
  let from_utf8_unchecked(move bytes: vec(u8)): string with(core.unsafe.unsafe_effect) = {
    string { bytes: bytes }
  }

  /// Returns the number of initialized UTF-8 bytes.
  let len_bytes(self: borrow(self))(): u64 = { self.bytes.len() }

  /// Returns the byte capacity of the backing allocation.
  let capacity(self: borrow(self))(): u64 = { self.bytes.capacity() }

  /// Returns whether this string contains no bytes.
  let is_empty(self: borrow(self))(): bool = { self.bytes.is_empty() }

  /// Borrows the initialized bytes without permitting mutation.
  let as_bytes(self: borrow(self))(): borrow(slice(u8)) = {
    self.bytes.as_slice(shared)()
  }

  /// Ensures capacity for at least `additional_bytes` more bytes.
  let reserve(self: borrow(mut)(self))(additional_bytes: u64): () = {
    self.bytes.reserve(additional_bytes)
  }

  /// Removes all text while retaining the allocation.
  let clear(self: borrow(mut)(self))(): () = { self.bytes.clear() }

  /// Moves the UTF-8 bytes from `other` onto the end of this string.
  let append(self: borrow(mut)(self))(other: borrow(mut)(string)): () = {
    self.bytes.append(other.bytes)
  }

  /// Consumes this string and returns its backing byte vector.
  let into_bytes(move self)(): vec(u8) = {
    let mut owner = self
    owner.bytes.take()
  }
}

extend from_utf8_error {
  /// Returns the byte length of the valid prefix before the malformed sequence.
  let valid_up_to(self: borrow(self))(): u64 = { self.valid_prefix }

  /// Consumes this error and recovers the original byte vector.
  let into_bytes(move self)(): vec(u8) = {
    let mut owner = self
    owner.bytes.take()
  }
}
