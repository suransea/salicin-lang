let pair(comptime t: type) = struct { first: bool, second: t }

let layout_sum(comptime t: type)(): u64 = { size_of(t) + align_of(t) }

let main(): i32 = {
  if layout_sum(pair(i64))() == 24 {
    42
  } else {
    0
  }
}

test("layout_intrinsics_generic.sc") {
  main() == 42
}
