let timespec = struct(c) {
  seconds: i64,
  nanoseconds: i64,
}

let header = struct(c) {
  timestamp: timespec,
  bytes: array(u8)(4),
  next: ptr(u8),
}

let pair(comptime t: type) = struct(c) {
  left: t,
  right: t,
}

let main(): i32 = {
  if size_of(timespec) == 16 &&
    align_of(timespec) == 8 &&
    size_of(header) == 32 &&
    align_of(header) == 8 &&
    size_of(pair(i32)) == 8 {
    42
  } else {
    0
  }
}

test("c_struct_layout.sc") {
  main() == 42
}
