extern "C" {
  @link_name("salicin_alloc")
  let allocator(size: u64, alignment: u64): Ptr(mut)(u8)
}

let main(): i32 = { 0 }
