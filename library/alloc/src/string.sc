let Option = core.Option
let Result = core.Result
let Slice = core.Slice
let Vec = alloc.vec.Vec

/// Owning, growable, well-formed UTF-8 text.
pub let String = struct {
  bytes: Vec(u8),
}

/// Failed consuming UTF-8 conversion with ownership of the original bytes.
pub let FromUtf8Error = struct {
  bytes: Vec(u8),
  valid_prefix: u64,
}

/// Returns the start of the first invalid UTF-8 sequence.
let utf8_invalid_at(bytes: borrow(Vec(u8))): Option(u64) = {
  let mut index: u64 = 0
  while { index < bytes.len() } {
    let first = bytes.read(index)
    if first <= 127 {
      index = index + 1
    } else if first >= 194 && first <= 223 {
      if index + 1 >= bytes.len() {
        return(Option(u64).Some(index))
      }
      let second = bytes.read(index + 1)
      if second < 128 || second > 191 {
        return(Option(u64).Some(index))
      }
      index = index + 2
    } else if first >= 224 && first <= 239 {
      if index + 2 >= bytes.len() {
        return(Option(u64).Some(index))
      }
      let second = bytes.read(index + 1)
      let third = bytes.read(index + 2)
      if second < 128 || second > 191 || third < 128 || third > 191 {
        return(Option(u64).Some(index))
      }
      if first == 224 && second < 160 {
        return(Option(u64).Some(index))
      }
      if first == 237 && second > 159 {
        return(Option(u64).Some(index))
      }
      index = index + 3
    } else if first >= 240 && first <= 244 {
      if index + 3 >= bytes.len() {
        return(Option(u64).Some(index))
      }
      let second = bytes.read(index + 1)
      let third = bytes.read(index + 2)
      let fourth = bytes.read(index + 3)
      if second < 128 || second > 191 || third < 128 || third > 191 || fourth < 128 || fourth > 191 {
        return(Option(u64).Some(index))
      }
      if first == 240 && second < 144 {
        return(Option(u64).Some(index))
      }
      if first == 244 && second > 143 {
        return(Option(u64).Some(index))
      }
      index = index + 4
    } else {
      return(Option(u64).Some(index))
    }
  }
  Option(u64).None
}

extend String {
  /// Creates an empty string with zero capacity.
  let new(): String = { String { bytes: Vec(u8).new() } }

  /// Creates an empty string with capacity for `byte_capacity` bytes.
  let with_capacity(byte_capacity: u64): String = {
    String { bytes: Vec(u8).with_capacity(byte_capacity) }
  }

  /// Validates and consumes an existing byte vector.
  let from_utf8(move bytes: Vec(u8)): Result(FromUtf8Error)(String) = {
    match utf8_invalid_at(bytes)
      { Some(valid_prefix) -> Result.Err(FromUtf8Error { bytes: bytes, valid_prefix: valid_prefix }) }
      { None -> Result.Ok(String { bytes: bytes }) }
  }

  /// Consumes bytes known by the caller to contain well-formed UTF-8.
  let from_utf8_unchecked(move bytes: Vec(u8)): String with(core.effect.Unsafe) = {
    String { bytes: bytes }
  }

  /// Returns the number of initialized UTF-8 bytes.
  let len_bytes(self: borrow(Self))(): u64 = { self.bytes.len() }

  /// Returns the byte capacity of the backing allocation.
  let capacity(self: borrow(Self))(): u64 = { self.bytes.capacity() }

  /// Returns whether this string contains no bytes.
  let is_empty(self: borrow(Self))(): bool = { self.bytes.is_empty() }

  /// Borrows the initialized bytes without permitting mutation.
  let as_bytes(self: borrow(Self))(): borrow(Slice(u8)) = {
    self.bytes.as_slice(shared)()
  }

  /// Ensures capacity for at least `additional_bytes` more bytes.
  let reserve(self: borrow(mut)(Self))(additional_bytes: u64): () = {
    self.bytes.reserve(additional_bytes)
  }

  /// Removes all text while retaining the allocation.
  let clear(self: borrow(mut)(Self))(): () = { self.bytes.clear() }

  /// Moves the UTF-8 bytes from `other` onto the end of this string.
  let append(self: borrow(mut)(Self))(other: borrow(mut)(String)): () = {
    self.bytes.append(other.bytes)
  }

  /// Consumes this string and returns its backing byte vector.
  let into_bytes(move self)(): Vec(u8) = {
    let mut owner = self
    owner.bytes.take()
  }
}

extend FromUtf8Error {
  /// Returns the byte length of the valid prefix before the malformed sequence.
  let valid_up_to(self: borrow(Self))(): u64 = { self.valid_prefix }

  /// Consumes this error and recovers the original byte vector.
  let into_bytes(move self)(): Vec(u8) = {
    let mut owner = self
    owner.bytes.take()
  }
}
