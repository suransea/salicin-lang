let signed_narrow(): bool = {
  let a: i8 = 120
  let b: i8 = 7
  let c: i16 = 30000
  a + b == 127 && c + 767 == 30767
}

let signed_wide(): bool = {
  let a: i128 = -170141183460469231731687303715884105728
  let b: isize = -9223372036854775808
  let minimum = match a
    { -170141183460469231731687303715884105728 -> true }
    { _ -> false }
  minimum && a + 1 < 0 && b / 2 == -4611686018427387904
}

let unsigned_narrow(): bool = {
  let a: u8 = 240
  let b: u16 = 65000
  (a | 15) == 255 && b + 535 == 65535
}

let unsigned_wide(): bool = {
  let maximum: u128 = 340282366920938463463374607431768211455
  let high: u128 = 170141183460469231731687303715884105728
  let pointer: usize = 65535
  let matched = match maximum
    { 340282366920938463463374607431768211455 -> true }
    { _ -> false }
  matched && maximum > high && (maximum >> 127) == 1 && pointer > 0
}

let layouts(): bool = {
  size_of(i8) == 1 &&
    size_of(u8) == 1 &&
    size_of(i16) == 2 &&
    size_of(u16) == 2 &&
    size_of(i32) == 4 &&
    size_of(u32) == 4 &&
    size_of(i64) == 8 &&
    size_of(u64) == 8 &&
    size_of(i128) == 16 &&
    size_of(u128) == 16 &&
    size_of(isize) == size_of(ptr(i8)) &&
    size_of(usize) == size_of(ptr(i8))
}

let main(): i32 = {
  if signed_narrow() && signed_wide() && unsigned_narrow() && unsigned_wide() && layouts() {
    42
  } else {
    0
  }
}

test("primitive_scalar_widths.sc") {
  main() == 42
}
