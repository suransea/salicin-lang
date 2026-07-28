/// Pure text parsing with a statically selected output and error type.
pub let parse = trait {
  /// Borrowed source type; standard parsing implementations bind this to
  /// `core.string.str`.
  let source: type
  /// Structured error reported for malformed or out-of-range input.
  let error: type

  /// Parses the complete borrowed text. Implementations do not allocate or
  /// accept leading or trailing input unless their concrete contract says so.
  let parse(comptime r: region)
    (value: borrow(r)(source)): core.result(error)(self)
}

/// Effect-polymorphic sink for validated UTF-8 fragments.
pub let text_writer(comptime e: effects) = trait {
  /// Writes one Unicode scalar without requiring a temporary allocation.
  let write_scalar(self: borrow(mut)(self))
    (value: core.string.unicode_scalar): () with(e)

  /// Writes one ASCII byte. Implementations trap for values above 127.
  let write_ascii(self: borrow(mut)(self))(value: u8): () with(e)
}

/// Stable categories for strict integer parsing failures.
pub let parse_int_error_kind = enum {
  empty,
  invalid_radix,
  invalid_sign,
  invalid_digit,
  overflow,
}

extend(parse_int_error_kind, core.marker.copyable) {}

/// A strict integer parsing failure and its first failing UTF-8 byte offset.
pub let parse_int_error = struct {
  failure: parse_int_error_kind,
  byte_offset: u64,
}

extend(parse_int_error) {
  let kind(self: borrow(self))(): parse_int_error_kind = { self.failure }
  let offset(self: borrow(self))(): u64 = { self.byte_offset }
}

let parse_failure(
  failure: parse_int_error_kind,
  byte_offset: u64,
): core.result(parse_int_error)(u64) = {
  core.result.err(parse_int_error {
    failure: failure,
    byte_offset: byte_offset,
  })
}

let digit_value(byte: u8): core.option(u8) = {
  if byte >= 48 && byte <= 57 {
    core.option.some(byte - 48)
  } else if byte >= 65 && byte <= 90 {
    core.option.some(byte - 65 + 10)
  } else if byte >= 97 && byte <= 122 {
    core.option.some(byte - 97 + 10)
  } else {
    core.option.none
  }
}

let byte_at(value: borrow(core.string.str), index: u64): u8 = {
  let bytes = value.as_bytes()
  let byte = bytes.at(index)
  byte
}

let widen_u8(value: u8): u64 = {
  let mut source = value
  let mut output: u64 = 0
  while { source != 0 } {
    source = source - 1
    output = output + 1
  }
  output
}

let parse_magnitude(
  value: borrow(core.string.str),
  start: u64,
  radix: u8,
  limit: u64,
): core.result(parse_int_error)(u64) = {
  if radix < 2 || radix > 36 {
    return(parse_failure(invalid_radix, 0))
  }
  if start == value.len() {
    return(parse_failure(empty, start))
  }
  let base = widen_u8(radix)
  let mut output: u64 = 0
  let mut index = start
  while { index < value.len() } {
    let digit = match digit_value(byte_at(value, index))
      { some(digit) -> digit }
      { none -> return(parse_failure(invalid_digit, index)) }
    if digit >= radix {
      return(parse_failure(invalid_digit, index))
    }
    let wide_digit = widen_u8(digit)
    if output > (limit - wide_digit) / base {
      return(parse_failure(overflow, index))
    }
    output = output * base + wide_digit
    index = index + 1
  }
  core.result.ok(output)
}

/// Parses an unsigned integer in radix 2 through 36.
pub let parse_u64_radix(
  value: borrow(core.string.str),
  radix: u8,
): core.result(parse_int_error)(u64) = {
  if value.is_empty() {
    return(core.result.err(parse_int_error {
      failure: empty,
      byte_offset: 0,
    }))
  }
  let first = byte_at(value, 0)
  if first == 43 || first == 45 {
    return(core.result.err(parse_int_error {
      failure: invalid_sign,
      byte_offset: 0,
    }))
  }
  match parse_magnitude(value, 0, radix, 18446744073709551615)
    { ok(magnitude) -> core.result.ok(magnitude) }
    { err(error) -> core.result.err(error) }
}

/// Parses a signed integer in radix 2 through 36.
pub let parse_i64_radix(
  value: borrow(core.string.str),
  radix: u8,
): core.result(parse_int_error)(i64) = {
  if radix < 2 || radix > 36 {
    return(core.result.err(parse_int_error {
      failure: invalid_radix,
      byte_offset: 0,
    }))
  }
  if value.is_empty() {
    return(core.result.err(parse_int_error {
      failure: empty,
      byte_offset: 0,
    }))
  }
  let first = byte_at(value, 0)
  let negative = first == 45
  let start: u64 = if negative || first == 43 { 1 } else { 0 }
  if start == value.len() {
    return(core.result.err(parse_int_error {
      failure: empty,
      byte_offset: start,
    }))
  }
  let mut base_source = radix
  let mut base: i64 = 0
  while { base_source != 0 } {
    base_source = base_source - 1
    base = base + 1
  }
  let mut output: i64 = 0
  let mut index = start
  while { index < value.len() } {
    let digit = match digit_value(byte_at(value, index))
      { some(digit) -> digit }
      { none -> return(core.result.err(parse_int_error {
          failure: invalid_digit,
          byte_offset: index,
        })) }
    if digit >= radix {
      return(core.result.err(parse_int_error {
        failure: invalid_digit,
        byte_offset: index,
      }))
    }
    let mut digit_source = digit
    let mut wide_digit: i64 = 0
    while { digit_source != 0 } {
      digit_source = digit_source - 1
      wide_digit = wide_digit + 1
    }
    if negative {
      if output < (-9223372036854775808 + wide_digit) / base {
        return(core.result.err(parse_int_error {
          failure: overflow,
          byte_offset: index,
        }))
      }
      output = output * base - wide_digit
    } else {
      if output > (9223372036854775807 - wide_digit) / base {
        return(core.result.err(parse_int_error {
          failure: overflow,
          byte_offset: index,
        }))
      }
      output = output * base + wide_digit
    }
    index = index + 1
  }
  core.result.ok(output)
}

extend(u64, parse) {
  let source = core.string.str
  let error = parse_int_error

  let parse(comptime r: region)
    (value: borrow(r)(source)): core.result(error)(self) = {
    parse_u64_radix(value, 10)
  }
}

extend(i64, parse) {
  let source = core.string.str
  let error = parse_int_error

  let parse(comptime r: region)
    (value: borrow(r)(source)): core.result(error)(self) = {
    parse_i64_radix(value, 10)
  }
}

let write_digit(comptime e: effects, comptime w: type)
  (writer: borrow(mut)(w))
  (digit: u8): () with(e)
where w: text_writer(e) = {
  writer.write_ascii(48 + digit)
}

let write_unsigned(comptime e: effects, comptime w: type)
  (writer: borrow(mut)(w))
  (value: u128): () with(e)
where w: text_writer(e) = {
  if value >= 10 {
    write_unsigned(writer)(value / 10)
  }
  let remainder = value % 10
  let digit: u8 = if remainder == 0 { 0 }
  else if remainder == 1 { 1 }
  else if remainder == 2 { 2 }
  else if remainder == 3 { 3 }
  else if remainder == 4 { 4 }
  else if remainder == 5 { 5 }
  else if remainder == 6 { 6 }
  else if remainder == 7 { 7 }
  else if remainder == 8 { 8 }
  else { 9 }
  write_digit(writer)(digit)
}

let write_minus(comptime e: effects, comptime w: type)
  (writer: borrow(mut)(w)): () with(e)
where w: text_writer(e) = {
  writer.write_ascii(45)
}

let display_unsigned(comptime e: effects, comptime w: type)
  (writer: borrow(mut)(w))
  (value: u128): () with(e)
where w: text_writer(e) = {
  write_unsigned(writer)(value)
}

let display_signed(comptime e: effects, comptime w: type)
  (writer: borrow(mut)(w))
  (negative: bool, magnitude: u128): () with(e)
where w: text_writer(e) = {
  if negative {
    write_minus(writer)
  }
  write_unsigned(writer)(magnitude)
}

let write_unsigned_u64(comptime e: effects, comptime w: type)
  (writer: borrow(mut)(w))
  (value: u64): () with(e)
where w: text_writer(e) = {
  if value >= 10 {
    write_unsigned_u64(writer)(value / 10)
  }
  let remainder = value % 10
  let digit: u8 = if remainder == 0 { 0 }
  else if remainder == 1 { 1 }
  else if remainder == 2 { 2 }
  else if remainder == 3 { 3 }
  else if remainder == 4 { 4 }
  else if remainder == 5 { 5 }
  else if remainder == 6 { 6 }
  else if remainder == 7 { 7 }
  else if remainder == 8 { 8 }
  else { 9 }
  write_digit(writer)(digit)
}

let display_signed_i64(comptime e: effects, comptime w: type)
  (writer: borrow(mut)(w))
  (negative: bool, magnitude: u64): () with(e)
where w: text_writer(e) = {
  if negative {
    write_minus(writer)
  }
  write_unsigned_u64(writer)(magnitude)
}

/// Writes the canonical lowercase boolean spelling.
pub let write_bool(comptime e: effects, comptime w: type)
  (writer: borrow(mut)(w))
  (value: bool): () with(e)
where w: text_writer(e) = {
  if value {
    writer.write_ascii(116)
    writer.write_ascii(114)
    writer.write_ascii(117)
    writer.write_ascii(101)
  } else {
    writer.write_ascii(102)
    writer.write_ascii(97)
    writer.write_ascii(108)
    writer.write_ascii(115)
    writer.write_ascii(101)
  }
}

extend(bool, display) {
  let display(comptime e: effects, comptime w: type)
    (self: borrow(self))
    (writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: bool = self
    write_bool(writer)(value)
  }
}

extend(core.string.unicode_scalar, display) {
  let display(comptime e: effects, comptime w: type)
    (self: borrow(self))
    (writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: core.string.unicode_scalar = self
    writer.write_scalar(value)
  }
}

extend(core.string.str, display) {
  let display(comptime e: effects, comptime w: type)
    (self: borrow(self))
    (writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let mut scalars = self.scalars()
    while {
      match scalars.next()
        { some(scalar) ->
          writer.write_scalar(scalar)
          true
        }
        { none -> false }
    } {}
  }
}

extend(core.string.string, display) {
  let display(comptime e: effects, comptime w: type)
    (self: borrow(self))
    (writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let view = self.as_str()
    let mut scalars = view.scalars()
    while {
      match scalars.next()
        { some(scalar) ->
          writer.write_scalar(scalar)
          true
        }
        { none -> false }
    } {}
  }
}

extend(u64, display) {
  let display(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: u64 = self
    write_unsigned_u64(writer)(value)
  }
}

extend(u128, display) {
  let display(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: u128 = self
    display_unsigned(writer)(value)
  }
}

extend(i64, display) {
  let display(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: i64 = self
    display_signed_i64(writer)(value < 0, value.magnitude())
  }
}

extend(i128, display) {
  let display(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: i128 = self
    display_signed(writer)(value < 0, value.magnitude())
  }
}

extend(bool, debug) {
  let debug(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: bool = self
    write_bool(writer)(value)
  }
}

extend(core.string.unicode_scalar, debug) {
  let debug(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: core.string.unicode_scalar = self
    writer.write_scalar(value)
  }
}

extend(core.string.str, debug) {
  let debug(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let mut scalars = self.scalars()
    while {
      match scalars.next()
        { some(scalar) ->
          writer.write_scalar(scalar)
          true
        }
        { none -> false }
    } {}
  }
}

extend(core.string.string, debug) {
  let debug(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let view = self.as_str()
    let mut scalars = view.scalars()
    while {
      match scalars.next()
        { some(scalar) ->
          writer.write_scalar(scalar)
          true
        }
        { none -> false }
    } {}
  }
}

extend(u64, debug) {
  let debug(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: u64 = self
    write_unsigned_u64(writer)(value)
  }
}

extend(u128, debug) {
  let debug(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: u128 = self
    write_unsigned(writer)(value)
  }
}

extend(i64, debug) {
  let debug(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: i64 = self
    display_signed_i64(writer)(value < 0, value.magnitude())
  }
}

extend(i128, debug) {
  let debug(comptime e: effects, comptime w: type)
    (self: borrow(self))(writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e) = {
    let value: i128 = self
    display_signed(writer)(value < 0, value.magnitude())
  }
}

/// Source-backed user-facing formatting.
pub let display = trait {
  /// Writes a deterministic display representation without reflection.
  let display(comptime e: effects, comptime w: type)
    (self: borrow(self))
    (writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e)
}

/// Source-backed diagnostic formatting.
pub let debug = trait {
  /// Writes a deterministic diagnostic representation without reflection.
  let debug(comptime e: effects, comptime w: type)
    (self: borrow(self))
    (writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e)
}
