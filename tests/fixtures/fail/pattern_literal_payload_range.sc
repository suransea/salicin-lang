let Value = enum { Number(u32), Empty }

let main(): i32 = { Value.Number(42) match {
  Number(-1) => 1,
  Number(_) => 2,
  Empty => 0,
}
}
