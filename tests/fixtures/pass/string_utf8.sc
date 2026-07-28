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

let accepts_utf8(bytes: borrow(core.memory.slice(u8)), expected_length: u64): bool = {
  match core.string.str.from_utf8(bytes)
    { some(text) ->
      let encoded = text.as_bytes()
      text.len() == expected_length &&
        text.is_empty() == (expected_length == 0) &&
        encoded.len() == expected_length
    }
    { none -> false }
}

let rejects_utf8(bytes: borrow(core.memory.slice(u8))): bool = {
  core.string.str.from_utf8(bytes).is_none()
}

let borrowed_text_checks(): bool = {
  let empty: array(u8)(0) = []
  let ascii: array(u8)(1) = [65]
  let two_byte: array(u8)(2) = [194, 128]
  let three_byte: array(u8)(3) = [224, 160, 128]
  let before_surrogates: array(u8)(3) = [237, 159, 191]
  let after_surrogates: array(u8)(3) = [238, 128, 128]
  let four_byte: array(u8)(4) = [240, 144, 128, 128]
  let maximum_scalar: array(u8)(4) = [244, 143, 191, 191]

  let continuation: array(u8)(1) = [128]
  let overlong_two: array(u8)(2) = [192, 128]
  let truncated_three: array(u8)(2) = [226, 130]
  let overlong_three: array(u8)(3) = [224, 128, 128]
  let surrogate: array(u8)(3) = [237, 160, 128]
  let overlong_four: array(u8)(4) = [240, 128, 128, 128]
  let above_unicode: array(u8)(4) = [244, 144, 128, 128]
  let invalid_lead: array(u8)(4) = [245, 128, 128, 128]

  let empty_view: borrow(core.memory.slice(u8)) = borrow(empty)
  let ascii_view: borrow(core.memory.slice(u8)) = borrow(ascii)
  let two_byte_view: borrow(core.memory.slice(u8)) = borrow(two_byte)
  let three_byte_view: borrow(core.memory.slice(u8)) = borrow(three_byte)
  let before_surrogates_view: borrow(core.memory.slice(u8)) = borrow(before_surrogates)
  let after_surrogates_view: borrow(core.memory.slice(u8)) = borrow(after_surrogates)
  let four_byte_view: borrow(core.memory.slice(u8)) = borrow(four_byte)
  let maximum_scalar_view: borrow(core.memory.slice(u8)) = borrow(maximum_scalar)
  let continuation_view: borrow(core.memory.slice(u8)) = borrow(continuation)
  let overlong_two_view: borrow(core.memory.slice(u8)) = borrow(overlong_two)
  let truncated_three_view: borrow(core.memory.slice(u8)) = borrow(truncated_three)
  let overlong_three_view: borrow(core.memory.slice(u8)) = borrow(overlong_three)
  let surrogate_view: borrow(core.memory.slice(u8)) = borrow(surrogate)
  let overlong_four_view: borrow(core.memory.slice(u8)) = borrow(overlong_four)
  let above_unicode_view: borrow(core.memory.slice(u8)) = borrow(above_unicode)
  let invalid_lead_view: borrow(core.memory.slice(u8)) = borrow(invalid_lead)

  accepts_utf8(empty_view, 0) &&
    accepts_utf8(ascii_view, 1) &&
    accepts_utf8(two_byte_view, 2) &&
    accepts_utf8(three_byte_view, 3) &&
    accepts_utf8(before_surrogates_view, 3) &&
    accepts_utf8(after_surrogates_view, 3) &&
    accepts_utf8(four_byte_view, 4) &&
    accepts_utf8(maximum_scalar_view, 4) &&
    rejects_utf8(continuation_view) &&
    rejects_utf8(overlong_two_view) &&
    rejects_utf8(truncated_three_view) &&
    rejects_utf8(overlong_three_view) &&
    rejects_utf8(surrogate_view) &&
    rejects_utf8(overlong_four_view) &&
    rejects_utf8(above_unicode_view) &&
    rejects_utf8(invalid_lead_view)
}

let string_view_checks(): bool = {
  let text: string = "柳"
  let view = text.as_str()
  let encoded = view.as_bytes()
  view.len() == 3 && !view.is_empty() && encoded.len() == 3
}

let subview_checks(): bool = {
  let text: string = "A柳𐀀"
  let ascii_expected: string = "A"
  let three_byte_expected: string = "柳"
  let four_byte_expected: string = "𐀀"
  let empty_text: string = ""
  let view = text.as_str()
  let ascii_expected_view = ascii_expected.as_str()
  let three_byte_expected_view = three_byte_expected.as_str()
  let four_byte_expected_view = four_byte_expected.as_str()
  let empty_view = empty_text.as_str()
  let boundaries =
    view.is_char_boundary(0) &&
    view.is_char_boundary(1) &&
    !view.is_char_boundary(2) &&
    !view.is_char_boundary(3) &&
    view.is_char_boundary(4) &&
    !view.is_char_boundary(5) &&
    !view.is_char_boundary(6) &&
    !view.is_char_boundary(7) &&
    view.is_char_boundary(8) &&
    !view.is_char_boundary(9)
  let valid = match view.get(0, 8)
    { some(whole) ->
      match view.get(0, 1)
      { some(ascii) ->
        match view.get(1, 4)
        { some(three_byte) ->
          match view.get(4, 8)
          { some(four_byte) ->
            match view.get(8, 8)
            { some(empty) ->
              whole == view &&
                ascii == ascii_expected_view &&
                three_byte == three_byte_expected_view &&
                four_byte == four_byte_expected_view &&
                empty.is_empty()
            }
            { none -> false }
          }
          { none -> false }
        }
        { none -> false }
      }
      { none -> false }
    }
    { none -> false }
  boundaries &&
    valid &&
    view.get(2, 4).is_none() &&
    view.get(1, 5).is_none() &&
    view.get(9, 9).is_none() &&
    view.get(4, 1).is_none() &&
    match empty_view.get(0, 0)
    { some(empty) -> empty.is_empty() }
    { none -> false }
}

let text_equality_checks(): bool = {
  let composed: string = "é"
  let same: string = "é"
  let decomposed: string = "é"
  let longer: string = "é!"
  let same_length_different: string = "ê"
  let view = composed.as_str()
  let same_view = same.as_str()
  let decomposed_view = decomposed.as_str()
    composed == same &&
    composed != decomposed &&
    composed != longer &&
    composed != same_length_different &&
    view == same_view &&
    view != decomposed_view
}

let main(): i32 = {
  let text = runtime_text()
  if text.len_bytes() == 7 &&
    greeting.len_bytes() == 3 &&
    scalar_checks() &&
    borrowed_text_checks() &&
    string_view_checks() &&
    subview_checks() &&
    text_equality_checks() {
    42
  } else {
    0
  }
}

test("string_utf8.sc") {
  main() == 42
}
