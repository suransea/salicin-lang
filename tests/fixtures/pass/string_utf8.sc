let greeting: string = "柳"

let runtime_text(): string = {
  "salicin"
}

let is_scalar(expected_value: u32, expected_length: u64): bool = {
  match core.string.unicode_scalar.from_u32(expected_value)
    { some(scalar) ->
      scalar.to_u32() == expected_value && scalar.len_utf8() == expected_length
    }
    { none -> false }
}

let scalar_checks(): bool = {
  let equality = match core.string.unicode_scalar.from_u32(65)
    { some(a) ->
      match core.string.unicode_scalar.from_u32(65)
        { some(another_a) ->
          match core.string.unicode_scalar.from_u32(66)
            { some(b) -> a == another_a && a != b }
            { none -> false }
        }
        { none -> false }
    }
    { none -> false }
  equality &&
    is_scalar(0, 1) &&
    is_scalar(127, 1) &&
    is_scalar(128, 2) &&
    is_scalar(2047, 2) &&
    is_scalar(2048, 3) &&
    is_scalar(55295, 3) &&
    is_scalar(57344, 3) &&
    is_scalar(65535, 3) &&
    is_scalar(65536, 4) &&
    is_scalar(1114111, 4) &&
    core.string.unicode_scalar.from_u32(55296).is_none() &&
    core.string.unicode_scalar.from_u32(57343).is_none() &&
    core.string.unicode_scalar.from_u32(1114112).is_none()
}

let main(): i32 = {
  let text = runtime_text()
  if text.len_bytes() == 7 && greeting.len_bytes() == 3 && scalar_checks() {
    42
  } else {
    0
  }
}

test("string_utf8.sc") {
  main() == 42
}
