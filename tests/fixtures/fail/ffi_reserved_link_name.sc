let allocator(
  size: u64,
  alignment: u64,
): Ptr(mut)(u8) = foreign(c, "salicin_alloc")

let main(): i32 = { 0 }
