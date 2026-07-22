let Pair(T: type) = struct { first: bool, second: T }

let layout_sum(T: type)(): u64 = { size_of(T) + align_of(T) }

let main(): i32 = { if layout_sum(Pair(i64))() == 24 {
  42
} else {
  0
}
}
