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
