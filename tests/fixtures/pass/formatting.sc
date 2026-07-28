let string = core.string.string
let parse_u64_radix = core.fmt.parse_u64_radix
let parse_i64_radix = core.fmt.parse_i64_radix
let string_writer = alloc.string.string_writer

let text_equal(left: borrow(string), right: borrow(string)): bool = {
  let left_view = left.as_str()
  let right_view = right.as_str()
  left_view == right_view
}

let parse_hex(): bool = {
  let source: string = "ff"
  let view = source.as_str()
  match parse_u64_radix(view, 16)
    { ok(value) -> value == 255 }
    { err(_) -> false }
}

let parse_minimum(): bool = {
  let source: string = "-9223372036854775808"
  let view = source.as_str()
  match parse_i64_radix(view, 10)
    { ok(value) -> value == -9223372036854775808 }
    { err(_) -> false }
}

let rejects_overflow(): bool = {
  let source: string = "18446744073709551616"
  let view = source.as_str()
  match parse_u64_radix(view, 10)
    { ok(_) -> false }
    { err(error) ->
      match error.kind()
      { overflow -> error.offset() == 19 }
      { _ -> false }
    }
}

let format_u64(value: u64): string = {
  let mut writer = string_writer.new()
  value.display(writer)
  writer.finish()
}

let format_i64(value: i64): string = {
  let mut writer = string_writer.new()
  value.display(writer)
  writer.finish()
}

let format_bool(value: bool): string = {
  let mut writer = string_writer.new()
  core.fmt.write_bool(writer)(value)
  writer.finish()
}

let format_scalar(value: core.string.unicode_scalar): string = {
  let mut writer = string_writer.new()
  value.display(writer)
  writer.finish()
}

let main(): i32 = {
  let maximum: u64 = 18446744073709551615
  let minimum: i64 = -9223372036854775808
  let truth: bool = true
  let decimal = format_u64(maximum)
  let expected_decimal: string = "18446744073709551615"
  let signed = format_i64(minimum)
  let expected_signed: string = "-9223372036854775808"
  let boolean = format_bool(truth)
  let expected_boolean: string = "true"
  let scalar = match core.string.unicode_scalar.from_u32(128578)
    { some(value) -> format_scalar(value) }
    { none -> "" }
  let expected_scalar: string = "🙂"
  if parse_hex() &&
    parse_minimum() &&
    rejects_overflow() &&
    text_equal(decimal, expected_decimal) &&
    text_equal(signed, expected_signed) &&
    text_equal(boolean, expected_boolean) &&
    text_equal(scalar, expected_scalar) {
    42
  } else {
    0
  }
}

test("formatting.sc") {
  main() == 42
}
