let i32_size: u64 = size_of(i32)

let padded = struct { flag: bool, value: i64 }

let main(): i32 = { if i32_size == 4 &&
    size_of(padded) == 16 &&
    align_of(padded) == 8 &&
    size_of(()) == 0 &&
    align_of(()) == 1 {
    42
  } else {
    0
  }
}

test("layout_intrinsics.sc") {
  main() == 42
}
