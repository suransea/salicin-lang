let scalar(value: u32): core.string.unicode_scalar = {
  match core.string.unicode_scalar.from_u32(value)
    { some(value) -> value }
    { none ->
      unsafe {
        raw_trap()
      }
    }
}

let construction_checks(): bool = {
  let source: string = "柳A"
  let source_view = source.as_str()
  let copied = string.from_str(source_view)
  let from_scalar = string.from_unicode_scalar(scalar(128578))
  let expected_scalar: string = "🙂"
  let empty = string.new()
  let reserved = string.with_capacity(12)
  copied == source &&
    copied.capacity() == 4 &&
    from_scalar == expected_scalar &&
    from_scalar.len_bytes() == 4 &&
    empty.is_empty() &&
    empty.capacity() == 0 &&
    reserved.is_empty() &&
    reserved.capacity() == 12
}

let append_checks(): bool = {
  let mut text: string = "A"
  text.reserve(7)
  let reserved = text.capacity() >= 8
  text.push(scalar(26611))
  let suffix: string = "🙂"
  let suffix_view = suffix.as_str()
  text.push_str(suffix_view)
  let expected: string = "A柳🙂"
  let view = text.as_str()
  let boundary = view.is_char_boundary(4)
  reserved &&
    text == expected &&
    text.len_bytes() == 8 &&
    text.capacity() >= text.len_bytes() &&
    boundary
}

let truncation_checks(): bool = {
  let source: string = "A柳🙂"
  let source_view = source.as_str()
  let mut text = string.from_str(source_view)
  let invalid = !text.truncate(2) && text.len_bytes() == 8
  let valid = text.truncate(4)
  let expected: string = "A柳"
  let unchanged = !text.truncate(9) && text == expected
  text.clear()
  invalid &&
    valid &&
    unchanged &&
    text.is_empty() &&
    text.capacity() == 8
}

let main(): i32 = {
  if construction_checks() &&
    append_checks() &&
    truncation_checks() {
    42
  } else {
    0
  }
}

test("string_mutation.sc") {
  main() == 42
}
