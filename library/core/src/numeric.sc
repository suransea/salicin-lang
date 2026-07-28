/// Three-way sign classification shared by every primitive integer.
pub let sign = enum {
  negative,
  zero,
  positive,
}

// These methods are source-defined wherever possible. `magnitude` for signed
// values and `checked_into` require representation-changing compiler support,
// but their complete public contracts remain declared here.

extend(i8) {
  let min(self)(other: i8): i8 = { if self < other { self } else { other } }
  let max(self)(other: i8): i8 = { if self > other { self } else { other } }
  let clamp(self)(minimum: i8, maximum: i8): i8 = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = {
    if self < 0 { negative }
    else if self > 0 { positive }
    else { zero }
  }
  let magnitude(self)(): u8 = builtin()
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(i16) {
  let min(self)(other: i16): i16 = { if self < other { self } else { other } }
  let max(self)(other: i16): i16 = { if self > other { self } else { other } }
  let clamp(self)(minimum: i16, maximum: i16): i16 = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = {
    if self < 0 { negative }
    else if self > 0 { positive }
    else { zero }
  }
  let magnitude(self)(): u16 = builtin()
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(i32) {
  let min(self)(other: i32): i32 = { if self < other { self } else { other } }
  let max(self)(other: i32): i32 = { if self > other { self } else { other } }
  let clamp(self)(minimum: i32, maximum: i32): i32 = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = {
    if self < 0 { negative }
    else if self > 0 { positive }
    else { zero }
  }
  let magnitude(self)(): u32 = builtin()
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(i64) {
  let min(self)(other: i64): i64 = { if self < other { self } else { other } }
  let max(self)(other: i64): i64 = { if self > other { self } else { other } }
  let clamp(self)(minimum: i64, maximum: i64): i64 = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = {
    if self < 0 { negative }
    else if self > 0 { positive }
    else { zero }
  }
  let magnitude(self)(): u64 = builtin()
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(i128) {
  let min(self)(other: i128): i128 = { if self < other { self } else { other } }
  let max(self)(other: i128): i128 = { if self > other { self } else { other } }
  let clamp(self)(minimum: i128, maximum: i128): i128 = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = {
    if self < 0 { negative }
    else if self > 0 { positive }
    else { zero }
  }
  let magnitude(self)(): u128 = builtin()
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(isize) {
  let min(self)(other: isize): isize = { if self < other { self } else { other } }
  let max(self)(other: isize): isize = { if self > other { self } else { other } }
  let clamp(self)(minimum: isize, maximum: isize): isize = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = {
    if self < 0 { negative }
    else if self > 0 { positive }
    else { zero }
  }
  let magnitude(self)(): usize = builtin()
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(u8) {
  let min(self)(other: u8): u8 = { if self < other { self } else { other } }
  let max(self)(other: u8): u8 = { if self > other { self } else { other } }
  let clamp(self)(minimum: u8, maximum: u8): u8 = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = { if self > 0 { positive } else { zero } }
  let magnitude(self)(): u8 = { self }
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(u16) {
  let min(self)(other: u16): u16 = { if self < other { self } else { other } }
  let max(self)(other: u16): u16 = { if self > other { self } else { other } }
  let clamp(self)(minimum: u16, maximum: u16): u16 = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = { if self > 0 { positive } else { zero } }
  let magnitude(self)(): u16 = { self }
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(u32) {
  let min(self)(other: u32): u32 = { if self < other { self } else { other } }
  let max(self)(other: u32): u32 = { if self > other { self } else { other } }
  let clamp(self)(minimum: u32, maximum: u32): u32 = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = { if self > 0 { positive } else { zero } }
  let magnitude(self)(): u32 = { self }
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(u64) {
  let min(self)(other: u64): u64 = { if self < other { self } else { other } }
  let max(self)(other: u64): u64 = { if self > other { self } else { other } }
  let clamp(self)(minimum: u64, maximum: u64): u64 = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = { if self > 0 { positive } else { zero } }
  let magnitude(self)(): u64 = { self }
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(u128) {
  let min(self)(other: u128): u128 = { if self < other { self } else { other } }
  let max(self)(other: u128): u128 = { if self > other { self } else { other } }
  let clamp(self)(minimum: u128, maximum: u128): u128 = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = { if self > 0 { positive } else { zero } }
  let magnitude(self)(): u128 = { self }
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}

extend(usize) {
  let min(self)(other: usize): usize = { if self < other { self } else { other } }
  let max(self)(other: usize): usize = { if self > other { self } else { other } }
  let clamp(self)(minimum: usize, maximum: usize): usize = {
    if maximum < minimum { unsafe { raw_trap() } }
    else if self < minimum { minimum }
    else if self > maximum { maximum }
    else { self }
  }
  let sign(self)(): sign = { if self > 0 { positive } else { zero } }
  let magnitude(self)(): usize = { self }
  let checked_into(comptime output: type)(self)(): core.option(output) = builtin()
}
