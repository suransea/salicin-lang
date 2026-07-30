let ctfe_minimum: i8 = -128
let ctfe_magnitude: u8 = ctfe_minimum.magnitude()
let ctfe_source: i16 = 255
let ctfe_conversion: core.option(u8) =
  ctfe_source.checked_into(output: u8)()

let is_some_u8(value: core.option(u8), expected: u8): bool = {
  match value
    { some(value) -> value == expected }
    { none -> false }
}

let is_some_i16(value: core.option(i16), expected: i16): bool = {
  match value
    { some(value) -> value == expected }
    { none -> false }
}

let is_none_u8(value: core.option(u8)): bool = {
  match value
    { some(_) -> false }
    { none -> true }
}

let is_none_i8(value: core.option(i8)): bool = {
  match value
    { some(_) -> false }
    { none -> true }
}

let sign_checks(): bool = {
  let negative: i32 = -1
  let zero: i32 = 0
  let positive: u32 = 1
  let negative_ok = match negative.sign()
    { negative -> true }
    { _ -> false }
  let zero_ok = match zero.sign()
    { zero -> true }
    { _ -> false }
  let positive_ok = match positive.sign()
    { positive -> true }
    { _ -> false }
  negative_ok && zero_ok && positive_ok
}

let conversion_checks(): bool = {
  let narrow: i16 = 255
  let overflow: i16 = 256
  let negative: i16 = -1
  let unsigned: u8 = 255
  let too_large: u16 = 32768
  is_some_u8(narrow.checked_into(output: u8)(), 255) &&
    is_none_u8(overflow.checked_into(output: u8)()) &&
    is_none_u8(negative.checked_into(output: u8)()) &&
    is_some_i16(unsigned.checked_into(output: i16)(), 255) &&
    is_none_i8(unsigned.checked_into(output: i8)()) &&
    match too_large.checked_into(output: i16)()
      { some(_) -> false }
      { none -> true }
}

let wide_checks(): bool = {
  let minimum: i128 = -170141183460469231731687303715884105728
  let maximum: u128 = 340282366920938463463374607431768211455
  let pointer_maximum: usize = 18446744073709551615
  let unsigned_to_signed = match maximum.checked_into(output: i128)()
    { some(_) -> false }
    { none -> true }
  let pointer_to_signed = match pointer_maximum.checked_into(output: isize)()
    { some(_) -> false }
    { none -> true }
  minimum.magnitude() == 170141183460469231731687303715884105728 &&
    unsigned_to_signed &&
    pointer_to_signed
}

let main(): i32 = {
  let value: i32 = 7
  let minimum: i32 = -3
  let maximum: i32 = 5
  if value.min(maximum) == 5 &&
    value.max(maximum) == 7 &&
    value.clamp(minimum, maximum) == 5 &&
    ctfe_magnitude == 128 &&
    is_some_u8(ctfe_conversion, 255) &&
    sign_checks() &&
    conversion_checks() &&
    wide_checks() {
    42
  } else {
    0
  }
}

test("numeric_utilities.sc") {
  std.test.assert(main() == 42)
}
