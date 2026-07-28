let vec = alloc.vec.vec

/// Owns bytes rejected by UTF-8 validation and the valid-prefix length.
pub let from_utf8_error = struct {
  bytes: vec(u8),
  valid_prefix: u64,
}

extend(from_utf8_error) {
  /// Returns the length of the valid UTF-8 prefix.
  let valid_up_to(self: borrow(self))(): u64 = { self.valid_prefix }

  /// Recovers ownership of the rejected bytes.
  let into_bytes(move self)(): vec(u8) = { self.bytes }
}

let first_invalid_owned_utf8(bytes: borrow(vec(u8))): core.option(u64) = {
  let source = bytes.as_slice(shared)()
  core.string.str.first_invalid_utf8(source)
}

/// Validates and consumes bytes, transferring their allocation on success and
/// returning the original owner on failure.
pub let string_from_utf8(
  move bytes: vec(u8),
): core.result(from_utf8_error)(core.string.string) = {
  match first_invalid_owned_utf8(bytes)
    { some(valid_up_to) ->
      core.result.err(from_utf8_error { bytes: bytes, valid_prefix: valid_up_to })
    }
    { none ->
      if bytes.is_empty() {
        core.result.ok("")
      } else {
        let parts = alloc.vec.vec_into_raw_parts(u8)(bytes)
        let text = unsafe {
          core.string.string_from_raw_parts(parts.0, parts.1, parts.2)
        }
        core.result.ok(text)
      }
    }
}

/// Consumes a string and returns owned bytes. Heap storage transfers without
/// copying; static literal storage is copied into a fresh vector.
pub let string_into_bytes(
  move value: core.string.string,
): vec(u8) = {
  let parts = unsafe {
    core.string.string_into_raw_parts(value)
  }
  if parts.2 == 0 {
    let mut bytes = vec(u8).with_capacity(parts.1)
    let mut index: u64 = 0
    while { index < parts.1 } {
      let byte = unsafe {
        *raw_offset(parts.0, index)
      }
      bytes.push(byte)
      index = index + 1
    }
    bytes
  } else {
    unsafe {
      alloc.vec.vec_from_raw_parts(u8)(parts.0, parts.1, parts.2)
    }
  }
}

/// Allocation-backed UTF-8 writer used by display and debug formatting.
pub let string_writer = struct {
  value: core.string.string,
}

extend(string_writer) {
  /// Creates an empty writer without allocating.
  let new(): string_writer = {
    string_writer { value: core.string.string.new() }
  }

  /// Creates an empty writer with reserved UTF-8 byte capacity.
  let with_capacity(capacity: u64): string_writer = {
    string_writer { value: core.string.string.with_capacity(capacity) }
  }

  /// Borrows the text written so far.
  let as_str(comptime r: region)
    (self: borrow(r)(self))(): borrow(r)(core.string.str) = {
    self.value.as_str()
  }

  /// Returns the completed owned string.
  let finish(move self)(): core.string.string = { self.value }
}

extend(string_writer, core.fmt.text_writer(pure)) {
  let write_scalar(self: borrow(mut)(self))
    (value: core.string.unicode_scalar): () = {
    self.value.push(value)
  }

  let write_ascii(self: borrow(mut)(self))(value: u8): () = {
    if value > 127 {
      unsafe {
        raw_trap()
      }
    }
    let mut source = value
    let mut code: u32 = 0
    while { source != 0 } {
      source = source - 1
      code = code + 1
    }
    match core.string.unicode_scalar.from_u32(code)
      { some(scalar) -> self.value.push(scalar) }
      { none -> unsafe { raw_trap() } }
  }
}
