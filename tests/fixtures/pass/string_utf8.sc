let result = std.result
let string = std.string.string
let vec = std.vec.vec

let valid(move bytes: vec(u8), expected_length: u64): bool = {
  match string.from_utf8(bytes)
    { ok(text) -> text.len_bytes() == expected_length }
    { err(_) -> false }
}

let invalid(move bytes: vec(u8), expected_prefix: u64, expected_length: u64): bool = {
  match string.from_utf8(bytes)
    { ok(_) -> false }
    { err(error) -> do {
        let prefix_matches = error.valid_up_to() == expected_prefix
        let recovered = error.into_bytes()
        prefix_matches && recovered.len() == expected_length
      }
    }
}

let ascii(): vec(u8) = {
  let mut bytes = vec(u8).new()
  bytes.push(65)
  bytes
}

let two_byte(): vec(u8) = {
  let mut bytes = vec(u8).new()
  bytes.push(194)
  bytes.push(162)
  bytes
}

let three_byte(): vec(u8) = {
  let mut bytes = vec(u8).new()
  bytes.push(226)
  bytes.push(130)
  bytes.push(172)
  bytes
}

let four_byte(): vec(u8) = {
  let mut bytes = vec(u8).new()
  bytes.push(240)
  bytes.push(159)
  bytes.push(152)
  bytes.push(128)
  bytes
}

let invalid_pair(first: u8, second: u8): vec(u8) = {
  let mut bytes = vec(u8).new()
  bytes.push(first)
  bytes.push(second)
  bytes
}

let invalid_three(first: u8, second: u8, third: u8): vec(u8) = {
  let mut bytes = vec(u8).new()
  bytes.push(first)
  bytes.push(second)
  bytes.push(third)
  bytes
}

let invalid_four(first: u8, second: u8, third: u8, fourth: u8): vec(u8) = {
  let mut bytes = vec(u8).new()
  bytes.push(first)
  bytes.push(second)
  bytes.push(third)
  bytes.push(fourth)
  bytes
}

let make_ascii(value: u8): string = {
  let mut bytes = vec(u8).new()
  bytes.push(value)
  match string.from_utf8(bytes)
    { ok(text) -> text }
    { err(_) -> string.new() }
}

let main(): i32 = {
  let accepted =
    valid(ascii(), 1) &&
    valid(two_byte(), 2) &&
    valid(three_byte(), 3) &&
    valid(four_byte(), 4)

  let rejected =
    invalid(invalid_pair(128, 128), 0, 2) &&
    invalid(invalid_pair(194, 65), 0, 2) &&
    invalid(invalid_pair(226, 130), 0, 2) &&
    invalid(invalid_three(224, 159, 128), 0, 3) &&
    invalid(invalid_three(237, 160, 128), 0, 3) &&
    invalid(invalid_four(240, 143, 128, 128), 0, 4) &&
    invalid(invalid_four(244, 144, 128, 128), 0, 4) &&
    invalid(invalid_four(245, 128, 128, 128), 0, 4)

  let mut left = make_ascii(65)
  let mut right = make_ascii(66)
  left.reserve(8)
  left.append(right)
  let appended = do {
    let bytes = left.as_bytes()
    left.len_bytes() == 2 &&
      left.capacity() >= 2 &&
      right.is_empty() &&
      bytes.len() == 2
  }

  left.clear()
  let cleared = left.is_empty() && left.capacity() >= 2
  let recovered = left.into_bytes()

  if accepted && rejected && appended && cleared && recovered.len() == 0 {
    42
  } else {
    0
  }
}

test("string_utf8.sc") {
  main() == 42
}
