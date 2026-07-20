let I32_SIZE: u64 = size_of(i32)

let Padded = struct(flag: bool, value: i64)

let main(): i32 = if I32_SIZE == 4 &&
  size_of(Padded) == 16 &&
  align_of(Padded) == 8 &&
  size_of(()) == 0 &&
  align_of(()) == 1 {
  42
} else {
  0
}
