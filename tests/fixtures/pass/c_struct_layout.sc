let Timespec = struct(c) {
  seconds: i64,
  nanoseconds: i64,
}

let Header = struct(c) {
  timestamp: Timespec,
  bytes: Array(u8)(4),
  next: Ptr(u8),
}

let Pair(T: type) = struct(c) {
  left: T,
  right: T,
}

let main(): i32 = {
  if size_of(Timespec) == 16 &&
    align_of(Timespec) == 8 &&
    size_of(Header) == 32 &&
    align_of(Header) == 8 &&
    size_of(Pair(i32)) == 8 {
    42
  } else {
    0
  }
}

test("c_struct_layout.sc") {
  main() == 42
}
